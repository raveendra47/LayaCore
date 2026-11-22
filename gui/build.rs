use std::env;
use std::fs;
use std::path::{Path, PathBuf};

/// Pure-Rust Windows `.ico` -> `.res` compiler for embedding `../assets/layacore.ico`
/// directly into `layacore.exe` with zero external build dependencies.
fn main() {
    let icon_path = Path::new("../assets/layacore.ico");
    println!("cargo:rerun-if-changed={}", icon_path.display());

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();

    if target_os != "windows" || target_env != "msvc" {
        return;
    }

    if !icon_path.exists() {
        return;
    }

    let ico_bytes = match fs::read(icon_path) {
        Ok(b) => b,
        Err(e) => {
            println!("cargo:warning=Could not read {}: {}", icon_path.display(), e);
            return;
        }
    };

    match build_windows_res_from_ico(&ico_bytes) {
        Ok(res_bytes) => {
            let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR not set"));
            let res_path = out_dir.join("layacore_gui.res");
            fs::write(&res_path, res_bytes).expect("Failed to write layacore_gui.res");
            println!("cargo:rustc-link-arg-bins={}", res_path.display());
        }
        Err(msg) => {
            println!("cargo:warning=Skipping Windows icon embedding: {}", msg);
        }
    }
}

fn push_res_record(out: &mut Vec<u8>, res_type: u16, res_id: u16, data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&32u32.to_le_bytes());
    out.extend_from_slice(&0xFFFFu16.to_le_bytes());
    out.extend_from_slice(&res_type.to_le_bytes());
    out.extend_from_slice(&0xFFFFu16.to_le_bytes());
    out.extend_from_slice(&res_id.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0x1030u16.to_le_bytes());
    out.extend_from_slice(&0x0409u16.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());

    out.extend_from_slice(data);

    let rem = data.len() % 4;
    if rem != 0 {
        for _ in 0..(4 - rem) {
            out.push(0);
        }
    }
}

fn build_windows_res_from_ico(ico: &[u8]) -> Result<Vec<u8>, String> {
    if ico.len() < 6 {
        return Err("ICO file too small".into());
    }

    let reserved = u16::from_le_bytes([ico[0], ico[1]]);
    let res_type = u16::from_le_bytes([ico[2], ico[3]]);
    let count = u16::from_le_bytes([ico[4], ico[5]]) as usize;

    if reserved != 0 || res_type != 1 || count == 0 {
        return Err("Invalid ICO header".into());
    }

    let dir_end = 6 + count * 16;
    if ico.len() < dir_end {
        return Err("Truncated ICO directory".into());
    }

    let mut res = Vec::with_capacity(ico.len() + 256);

    res.extend_from_slice(&0u32.to_le_bytes());
    res.extend_from_slice(&32u32.to_le_bytes());
    res.extend_from_slice(&0xFFFFu16.to_le_bytes());
    res.extend_from_slice(&0u16.to_le_bytes());
    res.extend_from_slice(&0xFFFFu16.to_le_bytes());
    res.extend_from_slice(&0u16.to_le_bytes());
    res.extend_from_slice(&0u32.to_le_bytes());
    res.extend_from_slice(&0u16.to_le_bytes());
    res.extend_from_slice(&0x0409u16.to_le_bytes());
    res.extend_from_slice(&0u32.to_le_bytes());
    res.extend_from_slice(&0u32.to_le_bytes());

    let mut grp_icon_dir = Vec::with_capacity(6 + count * 14);
    grp_icon_dir.extend_from_slice(&0u16.to_le_bytes());
    grp_icon_dir.extend_from_slice(&1u16.to_le_bytes());
    grp_icon_dir.extend_from_slice(&(count as u16).to_le_bytes());

    const RT_ICON: u16 = 3;
    const RT_GROUP_ICON: u16 = 14;

    for i in 0..count {
        let off = 6 + i * 16;
        let entry = &ico[off..off + 16];
        let bytes_in_res = u32::from_le_bytes([entry[8], entry[9], entry[10], entry[11]]) as usize;
        let image_offset = u32::from_le_bytes([entry[12], entry[13], entry[14], entry[15]]) as usize;

        if image_offset + bytes_in_res > ico.len() {
            return Err(format!("ICO entry {} out of bounds", i));
        }

        let icon_id = (i + 1) as u16;
        let img_data = &ico[image_offset..image_offset + bytes_in_res];

        push_res_record(&mut res, RT_ICON, icon_id, img_data);

        let mut grp_entry = [0u8; 12];
        grp_entry.copy_from_slice(&entry[0..12]);
        if u16::from_le_bytes([grp_entry[4], grp_entry[5]]) == 0 {
            grp_entry[4..6].copy_from_slice(&1u16.to_le_bytes());
        }
        if u16::from_le_bytes([grp_entry[6], grp_entry[7]]) == 0 {
            grp_entry[6..8].copy_from_slice(&32u16.to_le_bytes());
        }
        grp_icon_dir.extend_from_slice(&grp_entry);
        grp_icon_dir.extend_from_slice(&icon_id.to_le_bytes());
    }

    push_res_record(&mut res, RT_GROUP_ICON, 1, &grp_icon_dir);
    Ok(res)
}
