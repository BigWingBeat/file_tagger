use rfd::FileHandle;

pub async fn open_folder_as_db() -> Option<FileHandle> {
    rfd::AsyncFileDialog::new()
        .set_title("Open Folder As Database")
        .pick_folder()
        .await
}

pub async fn create_folder_and_db() -> Option<FileHandle> {
    rfd::AsyncFileDialog::new()
        .set_title("Create New Folder And Database")
        .save_file()
        .await
}

pub async fn import_files() -> Option<Vec<FileHandle>> {
    rfd::AsyncFileDialog::new()
        .set_title("Select Files to Import")
        .pick_files()
        .await
}
