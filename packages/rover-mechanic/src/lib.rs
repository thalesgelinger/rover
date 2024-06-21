pub fn gretting(name: &str) -> String {
    format!("Hello {}", name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = gretting("Rover");
        assert_eq!(result, "Hello Rover");
    }
}
