use std::fs;

pub fn clean(base_path: &str, full_clean: bool) {

    if full_clean {
        fs::remove_dir_all(format!("{}/repository", base_path)).ok();
    }
    
    fs::remove_dir_all(format!("{}/bin", base_path)).ok();
    fs::remove_dir_all(format!("{}/result", base_path)).ok();

    fs::create_dir(format!("{}/bin", base_path)).unwrap();
    fs::create_dir(format!("{}/result", base_path)).unwrap();
}
