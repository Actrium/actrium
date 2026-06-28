pub const PRODUCT_NAME: &str = "Actro";

#[cfg(test)]
mod tests {
    #[test]
    fn product_name_is_stable() {
        assert_eq!(super::PRODUCT_NAME, "Actro");
    }
}
