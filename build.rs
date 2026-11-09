fn main() {
    // A GitHub OAuth App client ID (public, not a secret) enables "Sign in with GitHub" via the
    // device flow. CI passes it from the OAUTH_CLIENT_ID repository variable.
    println!("cargo:rerun-if-env-changed=AKULAV_GITHUB_CLIENT_ID");

    // Embed the icon and version info into the Windows executable.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("ProductName", "Akulav Tracker");
        res.set("FileDescription", "Akulav Tracker - made by Akulav");
        res.set("CompanyName", "Akulav");
        res.set("LegalCopyright", "© Akulav. All rights reserved.");
        if let Err(e) = res.compile() {
            println!("cargo:warning=could not embed Windows resources: {e}");
        }
    }
}
