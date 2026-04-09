use std::fs;
use std::path::Path;

pub(crate) fn copy_dir_recursive(from: &Path, to: &Path) -> Result<(), std::io::Error> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let source = entry.path();
        let destination = to.join(entry.file_name());
        if source.is_dir() {
            copy_dir_recursive(&source, &destination)?;
        } else {
            fs::copy(source, destination)?;
        }
    }
    Ok(())
}
