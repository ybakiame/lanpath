use super::*;
use std::ffi::OsString;

#[cfg(windows)]
pub(crate) fn verify_running_bundle() -> LanpathResult<PathBuf> {
    let executable = std::env::current_exe()?;
    let directory = executable
        .parent()
        .ok_or_else(|| invalid("application executable has no directory"))?;
    if executable.file_name() != Some(std::ffi::OsStr::new(crate::product::BINARY_NAME)) {
        return Err(invalid("payload helper requires the product executable"));
    }
    let files = files::open_files(directory, false)?;
    let signed = SignedBundle::read(&files)?;
    signed.verify(&files, crate::product::UPDATE_PUBLIC_KEY)?;
    if signed.manifest(crate::product::UPDATE_PUBLIC_KEY)?.version != crate::product::VERSION {
        return Err(invalid(
            "running executable and signed bundle version differ",
        ));
    }
    Ok(directory.to_path_buf())
}

#[cfg(windows)]
pub(crate) fn admit_application() -> LanpathResult<()> {
    let executable = std::env::current_exe()?;
    let directory = executable
        .parent()
        .ok_or_else(|| invalid("application executable has no directory"))?;
    admit_application_at(directory)
}

#[derive(Debug, PartialEq, Eq)]
enum Command {
    Update { target: PathBuf, expected: OsString },
    Recover { target: PathBuf },
}

fn parse_command(mut args: impl Iterator<Item = OsString>) -> LanpathResult<Option<Command>> {
    let Some(command) = args.next() else {
        return Ok(None);
    };
    let replacing = match command.to_str() {
        Some("--lanpath-update-payload") => true,
        Some("--lanpath-recover-payload") => false,
        Some(
            "--lanpath-install-payload"
            | "--lanpath-recover-installed"
            | "--lanpath-update-portable"
            | "--lanpath-recover-portable",
        ) => return Err(invalid("unsupported GPUI payload command")),
        _ => return Ok(None),
    };
    let target = args
        .next()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| invalid("payload command requires an exact target directory"))?;
    let command = if replacing {
        let expected = args
            .next()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| invalid("payload update requires the exact signed bundle identity"))?;
        Command::Update {
            target: target.into(),
            expected,
        }
    } else {
        Command::Recover {
            target: target.into(),
        }
    };
    if args.next().is_some() {
        return Err(invalid("unexpected payload command arguments"));
    }
    Ok(Some(command))
}

pub(super) fn update_from_helper(
    target: &Path,
    helper: &Path,
    expected: &std::ffi::OsStr,
    key: &str,
) -> LanpathResult<()> {
    let source_files = files::open_files(helper, false)?;
    let bundle = SignedBundle::read(&source_files)?;
    let identity = signature::bundle_identity(&bundle.manifest(key)?)?;
    if expected != std::ffi::OsStr::new(&identity) {
        return Err(invalid(
            "payload differs from the exact signed update offer",
        ));
    }
    update_with(target, helper, key, |_| Ok(()))
}

pub(super) fn recover_from_helper(target: &Path, helper: &Path, key: &str) -> LanpathResult<()> {
    // A possibly mixed target cannot authenticate itself. Retain an independent
    // complete helper bundle's deny-write leases throughout recovery in BOTH modes.
    files::check_path(helper)?;
    let helper = helper.canonicalize()?;
    let source_files = files::open_files(&helper, false)?;
    SignedBundle::read(&source_files)?.verify(&source_files, key)?;
    let root = roots::admit(target)?;
    if helper == root.path || helper.starts_with(&root.path) || root.path.starts_with(&helper) {
        return Err(invalid(
            "payload recovery helper must be outside the selected root",
        ));
    }
    let _lock = lock(&root.path)?;
    recover_locked(&root, key).map(|_| ())
}

#[cfg(windows)]
pub(crate) fn dispatch_from_env() -> bool {
    let result = match parse_command(std::env::args_os().skip(1)) {
        Ok(None) => return false,
        Ok(Some(command)) => (|| match command {
            Command::Update { target, expected } => {
                let source = verify_running_bundle()?;
                update_from_helper(
                    &target,
                    &source,
                    expected.as_os_str(),
                    crate::product::UPDATE_PUBLIC_KEY,
                )
            }
            Command::Recover { target } => {
                let source = verify_running_bundle()?;
                recover_from_helper(&target, &source, crate::product::UPDATE_PUBLIC_KEY)
            }
        })(),
        Err(error) => Err(error),
    };
    if let Err(error) = result {
        eprintln!("GPUI payload transaction failed: {error}");
        std::process::exit(5);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn early_commands_require_exact_update_identity_and_no_extra_recovery_arguments() {
        let parse = |args: &[&str]| parse_command(args.iter().map(|arg| OsString::from(*arg)));
        assert_eq!(
            parse(&["--lanpath-update-payload", "target", "signed-id"]).unwrap(),
            Some(Command::Update {
                target: "target".into(),
                expected: "signed-id".into(),
            })
        );
        assert_eq!(
            parse(&["--lanpath-recover-payload", "target"]).unwrap(),
            Some(Command::Recover {
                target: "target".into()
            })
        );
        for args in [
            vec!["--lanpath-update-payload"],
            vec!["--lanpath-update-payload", "target"],
            vec!["--lanpath-update-payload", "target", ""],
            vec!["--lanpath-update-payload", "target", "signed-id", "extra"],
            vec!["--lanpath-recover-payload"],
            vec!["--lanpath-recover-payload", ""],
            vec!["--lanpath-recover-payload", "target", "unexpected"],
            vec!["--lanpath-install-payload", "target", "signed-id"],
            vec!["--lanpath-recover-installed", "target"],
            vec!["--lanpath-update-portable", "target", "signed-id"],
            vec!["--lanpath-recover-portable", "target"],
        ] {
            assert!(parse(&args).is_err());
        }
        assert!(parse(&[]).unwrap().is_none());
        assert!(parse(&["--unrelated-option"]).unwrap().is_none());
    }
}
