# octosense-theme-contract

Portable theme data, validation, reference/variant resolution, resolved revision
payloads, and trusted Makepad/CSS adapters. It has no dependency on an application,
renderer runtime, filesystem or network. See the [v1 format and integration guide](../../docs/customer-themes.md).

```sh
cargo test --locked --manifest-path crates/theme-contract/Cargo.toml
```

Consumers provide their resolved base roles, call `ThemePackage::parse` and
`resolve` for both variants, then validate/compile the complete result.
`ResolvedTheme::parse` verifies the complete payload and revision before host
delivery. `makepad::append_makepad` appends trusted definitions to a freshly
loaded platform stylesheet; do not repeatedly append to an already scaled sheet.
`css::variables` targets owned HTML only.
