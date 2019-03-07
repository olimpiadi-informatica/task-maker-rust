use pest::Parser;

#[derive(Parser)]
#[grammar = "task_types/ioi/GEN.pest"]
struct GENParser;

pub fn parse() {
    let content = "
# This is a comment
# N         T           S

# Subtask 1: Examples
#ST: 0
#COPY: testo/input0.txt

# Subtask 2: XXXX
#ST: 4
10          0           3
100         0           4

# Subtask 3: N <= 5000 OK/impossible
#ST: 7
50          2           7
500         1           8

# Subtask 4: OK/impossible
#ST: 9
10000       1           11
100000      2           12
";
    let file = GENParser::parse(Rule::file, &content);
    match file {
        Ok(mut file) => {
            let file = file.next().unwrap();
            for line in file.into_inner() {
                match line.as_rule() {
                    Rule::line => {
                        let line = line.into_inner().next().unwrap();
                        match line.as_rule() {
                            Rule::subtask => {
                                let score = line.into_inner().next().unwrap();
                                info!("Subtask! {:?}", score.as_str());
                            }
                            Rule::copy => {
                                let what = line.into_inner().next().unwrap();
                                info!("Copy! {:?}", what.as_str());
                            }
                            Rule::comment => info!("Comment!: {}", line.as_str()),
                            Rule::command => {
                                let cmd: Vec<String> =
                                    line.into_inner().map(|x| x.as_str().to_owned()).collect();
                                info!("Command! {:?}", cmd);
                            }
                            Rule::empty => info!("Empty!"),
                            _ => unreachable!(),
                        }
                    }
                    Rule::EOI => {}
                    _ => unreachable!(),
                }
            }
        }
        Err(e) => info!("{:#?}", e),
    }
}
