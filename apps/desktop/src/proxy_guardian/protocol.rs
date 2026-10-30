use lanpath_common::{LanpathError, LanpathResult, ProcessIdentity};
use std::ffi::OsString;

pub(super) fn failure(message: impl Into<String>) -> LanpathError {
    LanpathError::Client(format!("proxy_guardian: {}", message.into()))
}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct UserIdentity {
    pub sid: String,
    pub session: u32,
    pub logon: (u32, i32),
    pub elevated: bool,
}

pub(super) fn validate_user(parent: &UserIdentity, guardian: &UserIdentity) -> LanpathResult<()> {
    if parent.elevated || guardian.elevated || parent != guardian {
        return Err(failure(
            "guardian requires the same ordinary user and logon session",
        ));
    }
    Ok(())
}

pub(super) struct Arguments {
    pub owner: String,
    pub pid: u32,
    pub created: u64,
    pub parent: usize,
    pub pipe: String,
}

impl Arguments {
    pub fn parse(args: &[OsString]) -> LanpathResult<Self> {
        if args.len() != 6 || args[0] != "--proxy-guardian" {
            return Err(failure("invalid guardian arguments"));
        }
        let text = |index: usize| {
            args[index]
                .to_str()
                .ok_or_else(|| failure("invalid argument encoding"))
        };
        let owner = text(1)?;
        let id = uuid::Uuid::parse_str(owner).map_err(|_| failure("invalid watched owner"))?;
        if id.is_nil() || id.to_string() != owner {
            return Err(failure("invalid watched owner"));
        }
        let pid = text(2)?
            .parse()
            .map_err(|_| failure("invalid parent PID"))?;
        let created = text(3)?
            .parse()
            .map_err(|_| failure("invalid parent creation identity"))?;
        let handle = |index| -> LanpathResult<usize> {
            let value = text(index)?
                .parse::<usize>()
                .map_err(|_| failure("invalid inherited handle"))?;
            if value == 0 || value >= isize::MAX as usize {
                return Err(failure("invalid inherited handle"));
            }
            Ok(value)
        };
        let pipe = text(5)?;
        if !pipe
            .strip_prefix(r"\\.\pipe\LanpathGPUI-Proxy-")
            .is_some_and(|suffix| uuid::Uuid::parse_str(suffix).is_ok())
        {
            return Err(failure("invalid product pipe name"));
        }
        let parsed = Self {
            owner: owner.to_owned(),
            pid,
            created,
            parent: handle(4)?,
            pipe: pipe.to_owned(),
        };
        if pid == 0 || created == 0 {
            return Err(failure("invalid guardian process identity"));
        }
        Ok(parsed)
    }

    pub fn validate_parent(&self, actual: &ProcessIdentity) -> LanpathResult<()> {
        if actual.pid != self.pid || actual.created != self.created {
            return Err(failure("watched GUI process identity changed"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arguments() -> Vec<OsString> {
        [
            "--proxy-guardian",
            "727e41d5-a4d6-44ca-9184-5b8a4520a685",
            "123",
            "456",
            "4",
            r"\\.\pipe\LanpathGPUI-Proxy-51f4c1e1-40ad-4d1f-b4fc-d33f99819aa0",
        ]
        .into_iter()
        .map(OsString::from)
        .collect()
    }

    #[test]
    fn validates_creation_identity_not_only_pid() {
        let args = Arguments::parse(&arguments()).unwrap();
        assert_eq!(args.owner, "727e41d5-a4d6-44ca-9184-5b8a4520a685");
        assert!(args.pipe.starts_with(r"\\.\pipe\LanpathGPUI-Proxy-"));
        let mut parent = ProcessIdentity {
            pid: 123,
            created: 456,
            exe: "lanpath-gpui.exe".into(),
        };
        args.validate_parent(&parent).unwrap();
        parent.created += 1;
        assert!(args.validate_parent(&parent).is_err());
        parent.created = 456;
        parent.pid += 1;
        assert!(args.validate_parent(&parent).is_err());
    }

    #[test]
    fn rejects_unbounded_or_ambiguous_authority() {
        for (index, value) in [
            (1, ""),
            (1, "00000000-0000-0000-0000-000000000000"),
            (2, "0"),
            (3, "0"),
            (4, "0"),
            (4, "-1"),
            (5, r"\\.\pipe\arbitrary"),
        ] {
            let mut args = arguments();
            args[index] = value.into();
            assert!(Arguments::parse(&args).is_err());
        }
        let mut args = arguments();
        args.push("--command=anything".into());
        assert!(Arguments::parse(&args).is_err());
    }

    #[test]
    fn refuses_other_users_sessions_logons_and_inherited_elevation() {
        let parent = UserIdentity {
            sid: "S-1-5-21-test".into(),
            session: 1,
            logon: (42, 0),
            elevated: false,
        };
        validate_user(&parent, &parent).unwrap();
        for changed in [
            UserIdentity {
                sid: "other".into(),
                ..parent.clone()
            },
            UserIdentity {
                session: 2,
                ..parent.clone()
            },
            UserIdentity {
                logon: (43, 0),
                ..parent.clone()
            },
            UserIdentity {
                elevated: true,
                ..parent.clone()
            },
        ] {
            assert!(validate_user(&parent, &changed).is_err());
        }
        let elevated = UserIdentity {
            elevated: true,
            ..parent
        };
        assert!(validate_user(&elevated, &elevated).is_err());
    }
}
