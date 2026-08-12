mod clipboard {
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/clipboard.rs"));
}

use clipboard::{ClipEntry, ClipboardIO};

fn main() {
    let io = ClipboardIO::new();
    let html = "Version:0.9\r\nStartHTML:0000000100\r\nEndHTML:0000000340\r\nStartFragment:0000000150\r\nEndFragment:0000000300\r\n<html><body><!--StartFragment--><b>Bold and italic</b> formatted text<!--EndFragment--></body></html>\r\n";
    let entry = ClipEntry {
        text: "Bold and italic formatted text".to_string(),
        html: Some(html.to_string()),
        rtf: None,
        time: 0,
    };
    io.write(&entry, true);
    std::thread::sleep(std::time::Duration::from_millis(300));
    match io.read() {
        Some(e) => {
            println!("text: {}", e.text);
            match e.html {
                Some(h) => {
                    println!("html: {}", h);
                    if h.contains("StartFragment") && h.contains("Bold") {
                        println!("HTML_OK");
                    } else {
                        println!("HTML_BAD_CONTENT");
                    }
                }
                None => println!("HTML_MISSING"),
            }
        }
        None => println!("READ_FAILED"),
    }
}
