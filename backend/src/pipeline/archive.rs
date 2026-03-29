use std::path::Path;

use crate::types::ArchiveFormat;

pub fn detect_archive_format(filename: &str) -> Option<ArchiveFormat> {
    let lower = filename.to_lowercase();
    if lower.ends_with(".zip") {
        Some(ArchiveFormat::Zip)
    } else if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        Some(ArchiveFormat::TarGz)
    } else if lower.ends_with(".tar.bz2") || lower.ends_with(".tbz2") {
        Some(ArchiveFormat::TarBz2)
    } else if lower.ends_with(".tar.xz") || lower.ends_with(".txz") {
        Some(ArchiveFormat::TarXz)
    } else if lower.ends_with(".tar") {
        Some(ArchiveFormat::Tar)
    } else {
        None
    }
}

pub fn extract_zip(source: &Path, dest: &Path) -> Result<(), String> {
    let file = std::fs::File::open(source)
        .map_err(|e| format!("Failed to open zip file {:?}: {}", source, e))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| format!("Failed to read zip archive {:?}: {}", source, e))?;

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| format!("Failed to read zip entry {}: {}", i, e))?;

        let out_path = dest.join(entry.mangled_name());

        if entry.is_dir() {
            std::fs::create_dir_all(&out_path)
                .map_err(|e| format!("Failed to create directory {:?}: {}", out_path, e))?;
        } else {
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    format!("Failed to create parent directory {:?}: {}", parent, e)
                })?;
            }
            let mut outfile = std::fs::File::create(&out_path)
                .map_err(|e| format!("Failed to create file {:?}: {}", out_path, e))?;
            std::io::copy(&mut entry, &mut outfile)
                .map_err(|e| format!("Failed to extract {:?}: {}", out_path, e))?;

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Some(mode) = entry.unix_mode() {
                    let _ =
                        std::fs::set_permissions(&out_path, std::fs::Permissions::from_mode(mode));
                }
            }
        }
    }

    Ok(())
}

pub fn extract_tar_gz(source: &Path, dest: &Path) -> Result<(), String> {
    let file = std::fs::File::open(source)
        .map_err(|e| format!("Failed to open file {:?}: {}", source, e))?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    archive
        .unpack(dest)
        .map_err(|e| format!("Failed to extract tar.gz {:?}: {}", source, e))?;
    Ok(())
}

pub fn extract_tar_bz2(source: &Path, dest: &Path) -> Result<(), String> {
    let file = std::fs::File::open(source)
        .map_err(|e| format!("Failed to open file {:?}: {}", source, e))?;
    drop(file);
    let status = std::process::Command::new("tar")
        .args([
            "xjf",
            &source.to_string_lossy(),
            "-C",
            &dest.to_string_lossy(),
        ])
        .status()
        .map_err(|e| format!("Failed to run tar for bz2 extraction: {}", e))?;
    if !status.success() {
        return Err(format!("tar xjf failed with exit code {:?}", status.code()));
    }
    Ok(())
}

pub fn extract_tar_xz(source: &Path, dest: &Path) -> Result<(), String> {
    let file = std::fs::File::open(source)
        .map_err(|e| format!("Failed to open file {:?}: {}", source, e))?;
    let decoder = xz2::read::XzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    archive
        .unpack(dest)
        .map_err(|e| format!("Failed to extract tar.xz {:?}: {}", source, e))?;
    Ok(())
}

pub fn extract_tar(source: &Path, dest: &Path) -> Result<(), String> {
    let file = std::fs::File::open(source)
        .map_err(|e| format!("Failed to open file {:?}: {}", source, e))?;
    let mut archive = tar::Archive::new(file);
    archive
        .unpack(dest)
        .map_err(|e| format!("Failed to extract tar {:?}: {}", source, e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_archive_format() {
        assert!(matches!(
            detect_archive_format("file.zip"),
            Some(ArchiveFormat::Zip)
        ));
        assert!(matches!(
            detect_archive_format("file.tar.gz"),
            Some(ArchiveFormat::TarGz)
        ));
        assert!(matches!(
            detect_archive_format("file.tgz"),
            Some(ArchiveFormat::TarGz)
        ));
        assert!(matches!(
            detect_archive_format("file.tar.bz2"),
            Some(ArchiveFormat::TarBz2)
        ));
        assert!(matches!(
            detect_archive_format("file.tar.xz"),
            Some(ArchiveFormat::TarXz)
        ));
        assert!(matches!(
            detect_archive_format("file.tar"),
            Some(ArchiveFormat::Tar)
        ));
        assert!(detect_archive_format("file.txt").is_none());
    }

    #[test]
    fn test_detect_archive_format_case_insensitive() {
        assert!(matches!(
            detect_archive_format("FILE.ZIP"),
            Some(ArchiveFormat::Zip)
        ));
        assert!(matches!(
            detect_archive_format("Archive.TAR.GZ"),
            Some(ArchiveFormat::TarGz)
        ));
    }
}
