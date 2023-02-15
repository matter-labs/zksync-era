use std::path::Path;

pub struct TerminalSize {
    pub height: u32,
    pub width: u32,
}

pub struct App<'a> {
    pub terminal: TerminalSize,
    pub tokio: tokio::runtime::Runtime,
    pub db: zksync_dal::StorageProcessor<'a>,
}

pub fn create_app<'a>(profile: &Option<String>) -> Result<App<'a>, AppError> {
    if profile.is_some() {
        let home = std::env::var("ZKSYNC_HOME").map_err(|x| AppError::Init(InitError::Env(x)))?;

        let path =
            Path::new(home.as_str()).join(format!("etc/env/{}.env", profile.as_ref().unwrap()));

        dotenvy::from_filename(path)
            .map_err(|x| AppError::Init(InitError::Generic(x.to_string())))?;
    }

    let tokio = tokio::runtime::Runtime::new().map_err(|x| AppError::Init(InitError::IO(x)))?;

    let db =
        tokio.block_on(async { zksync_dal::StorageProcessor::establish_connection(true).await });

    let invocation = std::process::Command::new("stty")
        .arg("-f")
        .arg("/dev/stderr")
        .arg("size")
        .output();

    let terminal = match invocation {
        Result::Ok(x) if x.stderr.is_empty() => {
            let mut split = std::str::from_utf8(&x.stdout).unwrap().split_whitespace();

            TerminalSize {
                height: split.next().unwrap().parse().unwrap(),
                width: split.next().unwrap().parse().unwrap(),
            }
        }
        _ => TerminalSize {
            height: 60,
            width: 80,
        },
    };

    Ok(App {
        tokio,
        db,
        terminal,
    })
}

#[derive(Debug)]
pub enum InitError {
    Env(std::env::VarError),
    IO(std::io::Error),
    Generic(String),
}

#[derive(Debug)]
pub enum AppError {
    Db(String),
    Command(String),
    Init(InitError),
}
