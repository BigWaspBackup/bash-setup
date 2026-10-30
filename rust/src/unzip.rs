use std::{
    fs::{create_dir_all, File},
    io::{copy, Read, Result, Seek},
    path::{Path, PathBuf},
};

use zip::ZipArchive;

/// Extract a GitHub zipball into `target_dir`, dropping the top-level
/// `owner-repo-sha/` folder that GitHub adds.
pub fn unzip_strip_root<R: Read + Seek>(reader: R, target_dir: &Path) -> Result<()> {
    unzip_inner(reader, target_dir, true)
}

pub fn unzip<R: Read + Seek>(reader: R, target_dir: &Path) -> Result<()> {
    unzip_inner(reader, target_dir, false)
}

fn unzip_inner<R: Read + Seek>(reader: R, target_dir: &Path, strip_root: bool) -> Result<()> {
    let mut archive = ZipArchive::new(reader)?;
    create_dir_all(target_dir)?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let Some(path) = file.enclosed_name() else {
            continue;
        };

        let rel_path: PathBuf = if strip_root {
            path.components().skip(1).collect()
        } else {
            path.to_path_buf()
        };
        if rel_path.as_os_str().is_empty() {
            continue;
        }
        let outpath = target_dir.join(rel_path);

        if file.name().ends_with('/') {
            create_dir_all(&outpath)?;
        } else {
            if let Some(parent) = outpath.parent() {
                create_dir_all(parent)?;
            }
            let mut outfile = File::create(&outpath)?;
            copy(&mut file, &mut outfile)?;
        }
    }
    Ok(())
}

pub fn unzip_file_strip_root(zip_path: &Path, target_dir: &Path) -> std::result::Result<(), String> {
    let file = File::open(zip_path)
        .map_err(|e| format!("Failed to open {}: {e}", zip_path.display()))?;
    unzip_strip_root(file, target_dir)
        .map_err(|e| format!("Failed to extract {}: {e}", zip_path.display()))
}
