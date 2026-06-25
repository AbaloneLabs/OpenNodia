use std::env;

fn main() {
    let target = env::args().nth(1).unwrap_or_else(|| "approval".into());
    match target.as_str() {
        "approval" => print!("{}", opennodia_amm::contract::approval_program_source()),
        "clear" => print!("{}", opennodia_amm::contract::clear_state_source()),
        "registry-approval" => print!(
            "{}",
            opennodia_amm::contract::registry_approval_program_source()
        ),
        "registry-clear" => print!("{}", opennodia_amm::contract::clear_state_source()),
        other => {
            eprintln!("unknown contract target: {other}");
            std::process::exit(2);
        }
    }
}
