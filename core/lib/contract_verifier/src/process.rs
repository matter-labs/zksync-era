//! Process hardening and the execution boundary for untrusted compiler processes.
//!
//! This module owns subprocess I/O, lifecycle management, and OS-specific hardening so that
//! compiler request / response handling does not need to know about low-level process setup.

use std::process::Stdio;

use anyhow::Context as _;
use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWriteExt as _};

use crate::error::ContractVerifierError;

const MAX_COMPILER_STDOUT_BYTES: usize = 64 * 1024 * 1024;
const MAX_COMPILER_STDERR_BYTES: usize = 1024 * 1024;

/// Prevents same-UID compiler descendants from inspecting the verifier process on Linux.
pub(crate) fn harden_verifier_process() -> anyhow::Result<()> {
    #[cfg(target_os = "linux")]
    // Prevent compiler descendants running as the same UID from reading the verifier's memory or
    // `/proc/<pid>/environ`. Compiler commands receive an empty environment as a second layer.
    if unsafe { libc::prctl(libc::PR_SET_DUMPABLE, 0, 0, 0, 0) } == -1 {
        return Err(std::io::Error::last_os_error())
            .context("failed making contract verifier process non-dumpable");
    }
    Ok(())
}

/// Runs an untrusted compiler with a minimal environment, bounded output buffers, and a dedicated
/// process group. The process-group guard is deliberately cancellation-safe: when the outer
/// compilation timeout drops this future, the compiler and any descendants it spawned are killed.
/// On supported Linux architectures, the inherited seccomp policy prevents descendants from
/// leaving that process group.
pub(crate) async fn run_compiler(
    command: &mut tokio::process::Command,
    stdin: Option<&[u8]>,
) -> Result<std::process::Output, ContractVerifierError> {
    command
        .env_clear()
        .kill_on_drop(true)
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    harden_compiler_command(command);

    let mut child = command.spawn().context("failed spawning compiler")?;
    let process_group = child.id().map(ProcessGroupGuard);
    let child_stdin = child.stdin.take();
    let stdout = child
        .stdout
        .take()
        .context("compiler stdout is not piped")?;
    let stderr = child
        .stderr
        .take()
        .context("compiler stderr is not piped")?;

    let write_stdin = async move {
        if let (Some(content), Some(mut child_stdin)) = (stdin, child_stdin) {
            child_stdin
                .write_all(content)
                .await
                .context("failed writing compiler stdin")?;
            child_stdin
                .shutdown()
                .await
                .context("failed closing compiler stdin")?;
        }
        Ok::<_, ContractVerifierError>(())
    };
    let read_stdout = read_limited(stdout, MAX_COMPILER_STDOUT_BYTES);
    let read_stderr = read_limited(stderr, MAX_COMPILER_STDERR_BYTES);
    let wait = async {
        child
            .wait()
            .await
            .context("failed waiting for compiler")
            .map_err(ContractVerifierError::from)
    };

    let ((), stdout, stderr, status) =
        tokio::try_join!(write_stdin, read_stdout, read_stderr, wait)?;
    drop(process_group); // Also terminates descendants that survived the main compiler process.
    Ok(std::process::Output {
        status,
        stdout,
        stderr,
    })
}

async fn read_limited(
    reader: impl AsyncRead + Unpin,
    limit: usize,
) -> Result<Vec<u8>, ContractVerifierError> {
    let mut bytes = Vec::new();
    reader
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .await
        .context("failed reading compiler output")?;
    if bytes.len() > limit {
        return Err(ContractVerifierError::CompilerOutputTooLarge);
    }
    Ok(bytes)
}

fn harden_compiler_command(command: &mut tokio::process::Command) {
    // Linux establishes the process group in the `pre_exec` callback below, before installing the
    // seccomp filter that prevents the compiler and its descendants from changing it.
    #[cfg(all(unix, not(target_os = "linux")))]
    command.process_group(0);

    #[cfg(target_os = "linux")]
    {
        #[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
        let seccomp_filter = compiler_seccomp_filter();

        // SAFETY: `pre_exec` is limited to async-signal-safe libc calls. The seccomp program is
        // allocated in the parent and only borrowed by the callback.
        unsafe {
            command.pre_exec(move || {
                fn set_limit(
                    resource: libc::__rlimit_resource_t,
                    value: libc::rlim_t,
                ) -> std::io::Result<()> {
                    let limit = libc::rlimit {
                        rlim_cur: value,
                        rlim_max: value,
                    };
                    // SAFETY: `limit` points to a fully initialized `rlimit` value.
                    if unsafe { libc::setrlimit(resource, &limit) } == -1 {
                        return Err(std::io::Error::last_os_error());
                    }
                    Ok(())
                }

                // This must precede seccomp installation: once the filter is active, `setpgid()`
                // is intentionally unavailable to this process and all of its descendants.
                if libc::setpgid(0, 0) == -1 {
                    return Err(std::io::Error::last_os_error());
                }

                // Prevent privilege-gaining exec transitions and make compiler processes non-dumpable.
                if libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) == -1
                    || libc::prctl(libc::PR_SET_DUMPABLE, 0, 0, 0, 0) == -1
                {
                    return Err(std::io::Error::last_os_error());
                }
                libc::umask(0o077);
                set_limit(libc::RLIMIT_CORE, 0)?;
                set_limit(libc::RLIMIT_NOFILE, 64)?;
                set_limit(libc::RLIMIT_FSIZE, (64 * 1024 * 1024) as libc::rlim_t)?;
                // `RLIMIT_NPROC` is deliberately not set. Linux accounts it per real UID rather
                // than per process tree, so unrelated verifier and concurrent compiler threads
                // can make a seemingly generous per-compilation limit reject valid compilations.

                mark_inherited_fds_close_on_exec();
                #[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
                install_seccomp_filter(&seccomp_filter)?;
                Ok(())
            });
        }
    }
}

#[cfg(target_os = "linux")]
fn mark_inherited_fds_close_on_exec() {
    // `close_range(..., CLOSE_RANGE_CLOEXEC)` preserves Rust's exec-error reporting descriptor
    // until exec while ensuring DB sockets and any other inherited descriptors cannot reach the
    // compiler. Fall back to `fcntl` for older kernels / restrictive outer seccomp profiles.
    let result = unsafe {
        libc::syscall(
            libc::SYS_close_range,
            3_u32,
            u32::MAX,
            libc::CLOSE_RANGE_CLOEXEC,
        )
    };
    if result == -1 {
        for fd in 3..4096 {
            unsafe {
                libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC);
            }
        }
    }
}

#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
fn compiler_seccomp_filter() -> Vec<libc::sock_filter> {
    #[cfg(target_arch = "x86_64")]
    const AUDIT_ARCH: u32 = 0xc000_003e;
    #[cfg(target_arch = "aarch64")]
    const AUDIT_ARCH: u32 = 0xc000_00b7;

    fn statement(code: u32, value: u32) -> libc::sock_filter {
        libc::sock_filter {
            code: code as u16,
            jt: 0,
            jf: 0,
            k: value,
        }
    }

    fn jump(code: u32, value: u32, jt: u8, jf: u8) -> libc::sock_filter {
        libc::sock_filter {
            code: code as u16,
            jt,
            jf,
            k: value,
        }
    }

    let denied_syscalls = vec![
        // No networking, including use of socket pairs as covert IPC channels.
        libc::SYS_socket,
        libc::SYS_socketpair,
        libc::SYS_connect,
        libc::SYS_bind,
        libc::SYS_listen,
        libc::SYS_accept,
        libc::SYS_accept4,
        libc::SYS_sendto,
        libc::SYS_sendmsg,
        libc::SYS_sendmmsg,
        libc::SYS_recvfrom,
        libc::SYS_recvmsg,
        libc::SYS_recvmmsg,
        libc::SYS_shutdown,
        // Descendants must remain in the verifier-owned process group so the lifecycle guard can
        // reliably terminate the entire compiler process tree.
        libc::SYS_setpgid,
        libc::SYS_setsid,
        // No cross-process inspection or kernel attack-surface expansion.
        libc::SYS_ptrace,
        libc::SYS_process_vm_readv,
        libc::SYS_process_vm_writev,
        libc::SYS_pidfd_getfd,
        libc::SYS_bpf,
        libc::SYS_perf_event_open,
        libc::SYS_userfaultfd,
        libc::SYS_io_uring_setup,
        // The compiler only reads stdin and writes stdout / stderr. It must never mutate the
        // container filesystem, even if a compiler bug gives an attacker native execution.
        libc::SYS_openat2, // Its flags live behind a pointer and cannot be inspected by cBPF.
        libc::SYS_truncate,
        libc::SYS_ftruncate,
        libc::SYS_fallocate,
        libc::SYS_unlinkat,
        libc::SYS_renameat,
        libc::SYS_renameat2,
        libc::SYS_linkat,
        libc::SYS_symlinkat,
        libc::SYS_mkdirat,
        libc::SYS_mknodat,
        libc::SYS_fchmod,
        libc::SYS_fchmodat,
        libc::SYS_fchown,
        libc::SYS_fchownat,
        libc::SYS_setxattr,
        libc::SYS_lsetxattr,
        libc::SYS_fsetxattr,
        libc::SYS_removexattr,
        libc::SYS_lremovexattr,
        libc::SYS_fremovexattr,
        // No namespace, mount, keyring, or handle-based filesystem operations.
        libc::SYS_mount,
        libc::SYS_umount2,
        libc::SYS_pivot_root,
        libc::SYS_chroot,
        libc::SYS_unshare,
        libc::SYS_setns,
        libc::SYS_open_by_handle_at,
        libc::SYS_keyctl,
        libc::SYS_add_key,
        libc::SYS_request_key,
    ];
    #[cfg(target_arch = "x86_64")]
    let denied_syscalls = denied_syscalls
        .into_iter()
        .chain([
            // x86_64 retains legacy path-based syscalls that aarch64 never implemented.
            libc::SYS_creat,
            libc::SYS_unlink,
            libc::SYS_rename,
            libc::SYS_link,
            libc::SYS_symlink,
            libc::SYS_mkdir,
            libc::SYS_rmdir,
            libc::SYS_mknod,
            libc::SYS_chmod,
            libc::SYS_chown,
            libc::SYS_lchown,
        ])
        .collect::<Vec<_>>();

    let load_word = libc::BPF_LD | libc::BPF_W | libc::BPF_ABS;
    let jump_equal = libc::BPF_JMP | libc::BPF_JEQ | libc::BPF_K;
    let jump_bits_set = libc::BPF_JMP | libc::BPF_JSET | libc::BPF_K;
    let return_value = libc::BPF_RET | libc::BPF_K;
    let mut filter = Vec::with_capacity(15 + denied_syscalls.len() * 2);
    // Verify the syscall ABI before interpreting syscall numbers.
    filter.push(statement(load_word, 4)); // offsetof(seccomp_data, arch)
    filter.push(jump(jump_equal, AUDIT_ARCH, 1, 0));
    filter.push(statement(return_value, libc::SECCOMP_RET_KILL_PROCESS));
    filter.push(statement(load_word, 0)); // offsetof(seccomp_data, nr)

    #[cfg(target_arch = "x86_64")]
    {
        // x32 syscalls share AUDIT_ARCH_X86_64 but set bit 30 in the syscall number. Reject the
        // alternate ABI entirely so it cannot bypass the native syscall denylist below.
        const X32_SYSCALL_BIT: u32 = 0x4000_0000;
        filter.push(jump(jump_bits_set, X32_SYSCALL_BIT, 0, 1));
        filter.push(statement(
            return_value,
            libc::SECCOMP_RET_ERRNO | libc::EPERM as u32,
        ));
    }

    // `open()` / `openat()` remain available for read-only dynamic-loader and compiler accesses,
    // but all modes that can create or mutate a file fail with EPERM.
    let write_open_flags =
        libc::O_WRONLY | libc::O_RDWR | libc::O_CREAT | libc::O_TRUNC | libc::O_APPEND;
    let open_syscalls = vec![(libc::SYS_openat, 32_u32)]; // seccomp_data.args[2]
    #[cfg(target_arch = "x86_64")]
    let open_syscalls = open_syscalls
        .into_iter()
        .chain([(libc::SYS_open, 24_u32)]) // seccomp_data.args[1]
        .collect::<Vec<_>>();
    for (syscall, flags_offset) in open_syscalls {
        filter.push(jump(jump_equal, syscall as u32, 0, 4));
        filter.push(statement(load_word, flags_offset));
        filter.push(jump(jump_bits_set, write_open_flags as u32, 0, 1));
        filter.push(statement(
            return_value,
            libc::SECCOMP_RET_ERRNO | libc::EPERM as u32,
        ));
        filter.push(statement(load_word, 0)); // Reload the syscall number for following checks.
    }
    for syscall in denied_syscalls {
        filter.push(jump(jump_equal, syscall as u32, 0, 1));
        filter.push(statement(
            return_value,
            libc::SECCOMP_RET_ERRNO | libc::EPERM as u32,
        ));
    }
    filter.push(statement(return_value, libc::SECCOMP_RET_ALLOW));
    filter
}

#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
fn install_seccomp_filter(filter: &[libc::sock_filter]) -> std::io::Result<()> {
    let program = libc::sock_fprog {
        len: filter.len() as u16,
        filter: filter.as_ptr().cast_mut(),
    };
    let result = unsafe {
        libc::prctl(
            libc::PR_SET_SECCOMP,
            libc::SECCOMP_MODE_FILTER,
            &program as *const libc::sock_fprog as libc::c_ulong,
            0,
            0,
        )
    };
    if result == -1 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(unix)]
struct ProcessGroupGuard(u32);

#[cfg(unix)]
impl Drop for ProcessGroupGuard {
    fn drop(&mut self) {
        if let Ok(process_group) = i32::try_from(self.0) {
            // SAFETY: A negative PID addresses the dedicated process group created above.
            unsafe {
                libc::kill(-process_group, libc::SIGKILL);
            }
        }
    }
}

#[cfg(not(unix))]
struct ProcessGroupGuard(u32);

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    #[tokio::test]
    async fn compiler_process_receives_an_empty_environment() {
        let mut command = tokio::process::Command::new("/usr/bin/env");
        let output = super::run_compiler(&mut command, None).await.unwrap();
        assert!(output.status.success());
        assert!(
            output.stdout.is_empty(),
            "compiler inherited environment: {}",
            String::from_utf8_lossy(&output.stdout)
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn compiler_process_group_guard_kills_descendants() {
        let mut command = tokio::process::Command::new("/bin/sh");
        command.args(["-c", "/bin/sleep 60 >/dev/null 2>&1 & printf '%s' \"$!\""]);
        let output = super::run_compiler(&mut command, None).await.unwrap();
        assert!(output.status.success());
        let descendant_pid: i32 = String::from_utf8(output.stdout).unwrap().parse().unwrap();

        for _ in 0..100 {
            if !process_can_execute(descendant_pid) {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        panic!("compiler descendant {descendant_pid} survived process-group cleanup");
    }

    #[cfg(target_os = "linux")]
    fn process_can_execute(pid: i32) -> bool {
        // A container's PID 1 does not necessarily reap orphaned descendants promptly. A killed
        // process can therefore remain visible as a zombie even though it can no longer execute.
        let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
            return false;
        };
        let state = stat
            .rsplit_once(") ")
            .and_then(|(_, fields)| fields.chars().next());
        !matches!(state, Some('Z' | 'X') | None)
    }

    #[cfg(all(unix, not(target_os = "linux")))]
    fn process_can_execute(pid: i32) -> bool {
        (unsafe { libc::kill(pid, 0) }) != -1
    }

    #[cfg(all(
        target_os = "linux",
        any(target_arch = "x86_64", target_arch = "aarch64")
    ))]
    #[tokio::test]
    async fn compiler_seccomp_filter_denies_unsafe_operations() {
        let working_dir = tempfile::Builder::new()
            .prefix("contract-verifier-seccomp-probe-")
            .tempdir()
            .unwrap();
        let mut command = tokio::process::Command::new(std::env::current_exe().unwrap());
        command.current_dir(working_dir.path()).args([
            "--ignored",
            "--exact",
            "process::tests::seccomp_probe_helper",
        ]);
        let output = super::run_compiler(&mut command, None).await.unwrap();
        assert!(
            output.status.success(),
            "seccomp probe failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[cfg(all(
        target_os = "linux",
        any(target_arch = "x86_64", target_arch = "aarch64")
    ))]
    #[test]
    #[ignore = "spawned by compiler_seccomp_filter_denies_unsafe_operations"]
    fn seccomp_probe_helper() {
        let in_probe_dir = std::env::current_dir()
            .unwrap()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("contract-verifier-seccomp-probe-");
        if !in_probe_dir {
            return;
        }

        let err = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap_err();
        assert_eq!(err.raw_os_error(), Some(libc::EPERM));

        #[cfg(target_arch = "x86_64")]
        {
            const X32_SYSCALL_BIT: libc::c_long = 0x4000_0000;
            let result = unsafe { libc::syscall(X32_SYSCALL_BIT | libc::SYS_getpid) };
            assert_eq!(result, -1);
            assert_eq!(
                std::io::Error::last_os_error().raw_os_error(),
                Some(libc::EPERM)
            );
        }

        let err = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open("must-not-be-created")
            .unwrap_err();
        assert_eq!(err.raw_os_error(), Some(libc::EPERM));

        // The probe process is itself a process-group leader, so `setsid()` would fail even
        // without seccomp. Fork first to prove that descendants cannot escape the guarded group.
        assert_forked_syscall_is_denied(call_setsid);
        assert_forked_syscall_is_denied(call_setpgid);
    }

    #[cfg(all(
        target_os = "linux",
        any(target_arch = "x86_64", target_arch = "aarch64")
    ))]
    unsafe fn call_setsid() -> libc::c_int {
        // SAFETY: `setsid` takes no pointers or other caller-provided state.
        unsafe { libc::setsid() }
    }

    #[cfg(all(
        target_os = "linux",
        any(target_arch = "x86_64", target_arch = "aarch64")
    ))]
    unsafe fn call_setpgid() -> libc::c_int {
        // SAFETY: Zero PIDs select the calling process.
        unsafe { libc::setpgid(0, 0) }
    }

    #[cfg(all(
        target_os = "linux",
        any(target_arch = "x86_64", target_arch = "aarch64")
    ))]
    fn assert_forked_syscall_is_denied(syscall: unsafe fn() -> libc::c_int) {
        // SAFETY: The child calls only libc functions and exits immediately; it never returns to
        // the multithreaded Rust test harness.
        let child = unsafe { libc::fork() };
        assert_ne!(child, -1, "failed to fork seccomp probe");
        if child == 0 {
            // SAFETY: The supplied probes take no pointers and operate on the calling process.
            let result = unsafe { syscall() };
            // SAFETY: This helper only compiles on Linux, where `__errno_location` exposes the
            // calling thread's errno. `_exit` avoids running non-async-signal-safe Rust cleanup.
            let denied = result == -1 && unsafe { *libc::__errno_location() } == libc::EPERM;
            unsafe { libc::_exit(if denied { 0 } else { 1 }) };
        }

        let mut status = 0;
        // SAFETY: `child` is a positive PID returned by `fork`, and `status` is writable.
        assert_eq!(unsafe { libc::waitpid(child, &mut status, 0) }, child);
        assert!(libc::WIFEXITED(status));
        assert_eq!(libc::WEXITSTATUS(status), 0);
    }
}
