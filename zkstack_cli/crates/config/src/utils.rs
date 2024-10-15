use std::path::PathBuf;

use xshell::Shell;

// Find file in all parents repository and return necessary path or an empty error if nothing has been found
pub fn find_file(shell: &Shell, path_buf: PathBuf, file_name: &str) -> Result<PathBuf, ()> {
    let _dir = shell.push_dir(path_buf);
    if shell.path_exists(file_name) {
        Ok(shell.current_dir())
    } else {
        let current_dir = shell.current_dir();
        let Some(path) = current_dir.parent() else {
            return Err(());
        };
        find_file(shell, path.to_path_buf(), file_name)
    }
}
