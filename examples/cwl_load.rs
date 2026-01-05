use std::{env, fs};

use commonwl::documents::CWLDocument;

fn main() {
    let arg = env::args().nth(1).expect("Please provide one argument");
    let contents = fs::read_to_string(arg).unwrap();
    let result_doc = serde_yaml::from_str::<CWLDocument>(&contents);
    match result_doc {
        Ok(doc) => println!("Successfully loaded document: {:#?}", doc),
        Err(e) => eprintln!("Failed to load document: {}", e),
    }
}
