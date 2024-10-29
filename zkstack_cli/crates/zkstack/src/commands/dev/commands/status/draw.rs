use crate::{commands::dev::commands::status::utils::is_port_open, utils::ports::PortInfo};

const DEFAULT_LINE_WIDTH: usize = 32;

pub struct BoxProperties {
    longest_line: usize,
    border: String,
    boxed_msg: Vec<String>,
}

impl BoxProperties {
    fn new(msg: &str) -> Self {
        let longest_line = msg
            .lines()
            .map(|line| line.len())
            .max()
            .unwrap_or(0)
            .max(DEFAULT_LINE_WIDTH);
        let width = longest_line + 2;
        let border = "─".repeat(width);
        let boxed_msg = msg
            .lines()
            .map(|line| format!("│ {:longest_line$} │", line))
            .collect();
        Self {
            longest_line,
            border,
            boxed_msg,
        }
    }
}

fn single_bordered_box(msg: &str) -> String {
    let properties = BoxProperties::new(msg);
    format!(
        "┌{}┐\n{}\n└{}┘\n",
        properties.border,
        properties.boxed_msg.join("\n"),
        properties.border
    )
}

pub fn bordered_boxes(msg1: &str, msg2: Option<&String>) -> String {
    if msg2.is_none() {
        return single_bordered_box(msg1);
    }

    let properties1 = BoxProperties::new(msg1);
    let properties2 = BoxProperties::new(msg2.unwrap());

    let max_lines = properties1.boxed_msg.len().max(properties2.boxed_msg.len());
    let header = format!("┌{}┐  ┌{}┐\n", properties1.border, properties2.border);
    let footer = format!("└{}┘  └{}┘\n", properties1.border, properties2.border);

    let empty_line1 = format!(
        "│ {:longest_line$} │",
        "",
        longest_line = properties1.longest_line
    );
    let empty_line2 = format!(
        "│ {:longest_line$} │",
        "",
        longest_line = properties2.longest_line
    );

    let boxed_info: Vec<String> = (0..max_lines)
        .map(|i| {
            let line1 = properties1.boxed_msg.get(i).unwrap_or(&empty_line1);
            let line2 = properties2.boxed_msg.get(i).unwrap_or(&empty_line2);
            format!("{}  {}", line1, line2)
        })
        .collect();

    format!("{}{}\n{}", header, boxed_info.join("\n"), footer)
}

pub fn format_port_info(port_info: &PortInfo) -> String {
    let in_use_tag = if is_port_open(port_info.port) {
        " [OPEN]"
    } else {
        ""
    };

    format!(
        "  - {}{} > {}\n",
        port_info.port, in_use_tag, port_info.description
    )
}
