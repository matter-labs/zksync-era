use std::io::BufRead;

#[derive(Debug)]
pub struct IaiResult {
    pub name: String,
    pub instructions: u64,
    pub l1_accesses: u64,
    pub l2_accesses: u64,
    pub ram_accesses: u64,
    pub cycles: u64,
}

pub fn parse_iai<R: BufRead>(iai_output: R) -> impl Iterator<Item = IaiResult> {
    IaiResultParser {
        lines: iai_output.lines().map(|x| x.unwrap()),
    }
}

struct IaiResultParser<I: Iterator<Item = String>> {
    lines: I,
}

impl<I: Iterator<Item = String>> Iterator for IaiResultParser<I> {
    type Item = IaiResult;

    fn next(&mut self) -> Option<Self::Item> {
        self.lines.next().map(|name| {
            let result = IaiResult {
                name,
                instructions: self.parse_stat(),
                l1_accesses: self.parse_stat(),
                l2_accesses: self.parse_stat(),
                ram_accesses: self.parse_stat(),
                cycles: self.parse_stat(),
            };
            self.lines.next();
            result
        })
    }
}

impl<I: Iterator<Item = String>> IaiResultParser<I> {
    fn parse_stat(&mut self) -> u64 {
        let line = self.lines.next().unwrap();
        let number = line
            .split(':')
            .nth(1)
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap();
        number.parse().unwrap()
    }
}
