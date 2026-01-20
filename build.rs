fn main() {
    println!("cargo:rustc-link-search=/opt/homebrew/opt/postgresql@18/lib/postgresql");
    println!("cargo:rustc-link-lib=pq");
}
