const X: usize = 1589;

fn sub(x: i8, y: i8) -> i8 {
    let u = 8;
    x - y
}

#[test]
fn test_sub() {
    assert_eq!(sub(5, 2), 3)
}

#[tokio::test]
async fn async_test_sub() {
    assert_eq!(sub(5, 2), 3)
}
