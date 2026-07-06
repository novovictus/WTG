pub fn exit_code_for_status(status: &str) -> i32 {
    match status {
        "ok" => 0,
        "unavailable" => 2,
        "error" => 3,
        _ => 5,
    }
}
