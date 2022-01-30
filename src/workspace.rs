use std::fs;

pub fn clean(full_clean: bool) {

    if full_clean {
        fs::remove_dir_all("workspace/repository").ok();
    }
    
    fs::remove_dir_all("workspace/bin").ok();
    fs::remove_dir_all("workspace/result").ok();

    fs::create_dir("workspace/bin").unwrap();
    fs::create_dir("workspace/result").unwrap();
}
