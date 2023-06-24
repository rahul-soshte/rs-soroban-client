pub fn placeholder_fn(left: usize, right: usize) -> usize {
    left + right
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = placeholder_fn(2, 2);
        assert_eq!(result, 4);
    }
}
