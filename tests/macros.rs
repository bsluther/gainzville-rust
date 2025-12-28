/// Helper macro for async proptest tests
///
/// Automatically configures FileFailurePersistence for integration tests.
///
/// Usage:
/// ```
/// async_proptest! {
///     fn test_name(user in any::<User>(), count in 1..100u32) {
///         let pool = setup_pool().await;
///         // ... async test code
///     }
/// }
/// ```
#[macro_export]
macro_rules! async_proptest {
    (
        $(#[$meta:meta])*
        fn $name:ident($($arg:ident in $strategy:expr),+ $(,)?) $body:block
    ) => {
        proptest::proptest! {
            #![proptest_config(proptest::prelude::ProptestConfig::with_failure_persistence(
                proptest::test_runner::FileFailurePersistence::WithSource("regressions")
            ))]

            $(#[$meta])*
            fn $name($($arg in $strategy),+) {
                tokio::runtime::Runtime::new()
                    .unwrap()
                    .block_on(async $body);
            }
        }
    };
}
