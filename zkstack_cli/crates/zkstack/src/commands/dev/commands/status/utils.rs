use std::net::TcpListener;

pub fn is_port_open(port: u16) -> bool {
    TcpListener::bind(("0.0.0.0", port)).is_err() || TcpListener::bind(("127.0.0.1", port)).is_err()
}

pub fn deslugify(name: &str) -> String {
    name.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => {
                    let capitalized = first.to_uppercase().collect::<String>() + chars.as_str();
                    match capitalized.as_str() {
                        "Http" => "HTTP".to_string(),
                        "Api" => "API".to_string(),
                        "Ws" => "WS".to_string(),
                        _ => capitalized,
                    }
                }
                None => String::new(),
            }
        })
        .collect::<Vec<String>>()
        .join(" ")
}
