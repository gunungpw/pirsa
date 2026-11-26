fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");

    #[cfg(windows)]
    {
        let mut resources = winresource::WindowsResource::new();
        resources.set_icon("assets/icon.ico");
        resources.set("ProductName", "Pirsa");
        resources.set("FileDescription", "Pirsa");
        resources.set("CompanyName", "Pirsa contributors");
        resources.set("LegalCopyright", "MIT License");
        resources.set("OriginalFilename", "pirsa.exe");
        resources.set("ProductVersion", env!("CARGO_PKG_VERSION"));
        resources.set("FileVersion", env!("CARGO_PKG_VERSION"));

        resources
            .compile()
            .expect("failed to compile Windows icon resources");
    }
}
