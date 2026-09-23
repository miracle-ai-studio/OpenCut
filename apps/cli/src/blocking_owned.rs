//! Cancellation-safe ownership of synchronous native resources. Each blocking
//! operation owns its resource until it finishes; cancelling the async waiter
//! cannot release a reservation while a detached operation still writes.
use opencut_contracts::EditorError;

pub(crate) struct BlockingOwned<T: Send + 'static>(Option<T>);
fn join(error: tokio::task::JoinError) -> EditorError {
    EditorError::new(
        "INTERNAL_ERROR",
        format!("Native operation failed: {error}"),
    )
}
impl<T: Send + 'static> BlockingOwned<T> {
    pub(crate) async fn create(
        make: impl FnOnce() -> Result<T, EditorError> + Send + 'static,
    ) -> Result<Self, EditorError> {
        tokio::task::spawn_blocking(move || make().map(|value| Self(Some(value))))
            .await
            .map_err(join)?
    }
    pub(crate) async fn run<R: Send + 'static>(
        mut self,
        operation: impl FnOnce(&mut T) -> Result<R, EditorError> + Send + 'static,
    ) -> Result<(Self, R), EditorError> {
        tokio::task::spawn_blocking(move || {
            let result = operation(self.0.as_mut().expect("owned resource"))?;
            Ok((self, result))
        })
        .await
        .map_err(join)?
    }
}
impl<T: Send + 'static> Drop for BlockingOwned<T> {
    fn drop(&mut self) {
        if let Some(value) = self.0.take() {
            if let Ok(runtime) = tokio::runtime::Handle::try_current() {
                runtime.spawn_blocking(move || drop(value));
            } else {
                drop(value);
            }
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use opencut_editor_application::budget::BudgetNative;
    use opencut_native_services::budget::{AdmittedFile, NativeBudget};

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cancelled_waiter_keeps_real_file_and_reservation_until_native_write_finishes() {
        let root = tempfile::tempdir().unwrap();
        let database = root.path().join("workspace.sqlite");
        let media = root.path().join("media");
        std::fs::create_dir(&media).unwrap();
        let path = media.join(".upload-cancellation");
        let (db, temporary) = (database.clone(), path.clone());
        let file = BlockingOwned::create(move || {
            AdmittedFile::create(&db, &temporary, "media_upload", 8, false)
        })
        .await
        .unwrap();
        let (started, running) = tokio::sync::oneshot::channel();
        let (release, fence) = std::sync::mpsc::channel();
        let task = tokio::spawn(file.run(move |file| {
            started.send(()).unwrap();
            fence
                .recv_timeout(std::time::Duration::from_secs(10))
                .unwrap();
            file.append(b"12345678")?;
            file.sync()
        }));
        running.await.unwrap();
        task.abort();
        match task.await {
            Err(error) => assert!(error.is_cancelled()),
            Ok(_) => panic!("Waiter must be cancelled"),
        }
        let active = NativeBudget(&database).inspect(true).unwrap();
        assert_eq!(active.reserved_bytes, "8");
        assert_eq!(active.recovered_reservations, 0);
        assert!(path.exists());
        release.send(()).unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            while NativeBudget(&database)
                .inspect(false)
                .unwrap()
                .reserved_bytes
                != "0"
            {
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        assert!(!path.exists());
    }
}
