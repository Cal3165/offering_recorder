use std::path::Path;

use offering_application::error::AppResult;

use crate::{backend::validation, workspace::ResultDocument};

pub fn import_csv(directory: &Path) -> AppResult<Option<tempfile::TempDir>> {
    let stage = tempfile::tempdir_in(directory)
        .map_err(|_| validation("Could not prepare private storage for the CSV."))?;
    if pick(&stage.path().join("Imported donors.csv"), "*/*", false)? {
        Ok(Some(stage))
    } else {
        Ok(None)
    }
}

pub fn export(directory: &Path, document: &ResultDocument) -> AppResult<bool> {
    let stage = tempfile::tempdir_in(directory)
        .map_err(|_| validation("Could not prepare private storage for the export."))?;
    let path = stage
        .path()
        .join(format!("Offering Recorder.{}", document.extension));
    std::fs::write(&path, &document.export)
        .map_err(|_| validation("Could not stage the report for export."))?;
    let mime = match document.extension {
        "json" => "application/json",
        _ => "text/plain",
    };
    pick(&path, mime, true)
}

fn pick(path: &Path, mime: &str, export: bool) -> AppResult<bool> {
    let path = path
        .to_str()
        .ok_or_else(|| validation("Invalid staging path."))?;
    match native::pick(path, mime, export) {
        1 => Ok(true),
        0 => Ok(false),
        -2 => Err(validation("The CSV exceeds the 5 MiB limit.")),
        _ => Err(validation(
            "Could not access the document. Choose an accessible file or location and try again.",
        )),
    }
}

#[cfg(target_os = "android")]
pub mod native {
    use jni::{
        objects::{JObject, JValue},
        JavaVM,
    };
    use slint::android::AndroidApp;
    use std::{path::Path, sync::Mutex};

    static ACTIVITY: Mutex<Option<AndroidApp>> = Mutex::new(None);

    pub struct ActivityGuard;

    impl ActivityGuard {
        pub fn register(app: AndroidApp) -> Self {
            *ACTIVITY.lock().unwrap() = Some(app);
            Self
        }
    }

    impl Drop for ActivityGuard {
        fn drop(&mut self) {
            ACTIVITY.lock().unwrap().take();
        }
    }

    pub(super) fn pick(path: &str, mime: &str, export: bool) -> i32 {
        let Some(app) = ACTIVITY.lock().unwrap().clone() else {
            return -1;
        };
        invoke(app, path, mime, export).unwrap_or(-1)
    }

    fn invoke(app: AndroidApp, path: &str, mime: &str, export: bool) -> jni::errors::Result<i32> {
        // AndroidApp keeps its unowned JNI references valid for this call.
        let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) }?;
        let mut env = vm.attach_current_thread()?;
        let activity = unsafe { JObject::from_raw(app.activity_as_ptr().cast()) };
        let filename = Path::new(path)
            .file_name()
            .and_then(|p| p.to_str())
            .unwrap_or("Offering Recorder");
        let path = env.new_string(path)?;
        let filename = env.new_string(filename)?;
        let mime = env.new_string(mime)?;
        env.call_method(
            &activity,
            "pickDocument",
            "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Z)I",
            &[
                JValue::Object(&path),
                JValue::Object(&filename),
                JValue::Object(&mime),
                JValue::Bool(export.into()),
            ],
        )?
        .i()
    }
}

#[cfg(target_os = "ios")]
mod native {
    use std::ffi::{c_char, c_int, CString};

    extern "C" {
        fn offering_pick_document(path: *const c_char, export: c_int) -> c_int;
    }

    pub(super) fn pick(path: &str, _mime: &str, export: bool) -> i32 {
        let Ok(path) = CString::new(path) else {
            return -1;
        };
        // The picker call is synchronous on the worker and does not retain the pointer.
        unsafe { offering_pick_document(path.as_ptr(), i32::from(export)) }
    }
}
