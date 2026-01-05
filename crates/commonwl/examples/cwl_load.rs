use std::env;
use commonwl::load_cwl_file;

fn main() {
    let arg = env::args().nth(1).expect("Please provide one argument");
    let result_doc = load_cwl_file(arg, true);
    match result_doc {
        Ok(doc) => println!("Successfully loaded document: {:#?}", doc),
        Err(e) => eprintln!("Failed to load document: {}", e),
    }
}
