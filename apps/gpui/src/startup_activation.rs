use std::path::Path;

use crate::{
    data_root::{DataLocation, DataRoot},
    distribution::instance,
};

pub(super) fn activate_existing() -> Result<bool, String> {
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let explicit = std::env::var_os("LANPATH_DESKTOP_DATA_DIR");
    let location = match DataLocation::resolve(&executable, explicit.as_deref().map(Path::new)) {
        Ok(location) => location,
        // The normal runtime reports admission errors in its configuration state.
        // An error is never treated as proof that another instance is running.
        Err(_) => return Ok(false),
    };
    match DataRoot::is_in_use(&location.path) {
        Ok(true) => {
            instance::activate_existing(&location.path).map_err(|error| error.to_string())?;
            Ok(true)
        }
        Ok(false) | Err(_) => Ok(false),
    }
}
