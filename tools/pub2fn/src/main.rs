fn main() {
    println!("Hello, world!");
}

#[cfg(test)]
mod test {
    #[test]
    fn fail() {
        assert!(false);
    }
}
