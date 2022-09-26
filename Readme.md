[![Crates.io](https://img.shields.io/crates/v/actix-web-query-method-middleware)](https://crates.io/crates/actix-web-query-method-middleware)
[![docs.rs](https://img.shields.io/docsrs/actix-web-query-method-middleware)](https://docs.rs/actix-web-query-method-middleware/latest/actix_web_query_method_middleware/)
[![GitHub Workflow Status](https://img.shields.io/github/workflow/status/SeriousBug/actix-web-query-method-middleware/tests?label=tests)](https://github.com/SeriousBug/actix-web-query-method-middleware/actions/workflows/test.yml)
[![Codecov](https://img.shields.io/codecov/c/github/SeriousBug/actix-web-query-method-middleware?color=green)](https://app.codecov.io/gh/SeriousBug/actix-web-query-method-middleware)

An Actix Web middleware that allows you to reroute `POST` requests to other
methods like `PUT` or `DELETE` using a query parameter.

This is useful in HTML forms where you can't use methods other than `GET` or
`POST`. By adding this middleware to your server, you can submit the form to
endpoints with methods other than `POST` by adding a query parameter like
`/your/url?_method=PUT`.

For example:

```html
<form method="post" action="/path/to/endpoint?_method=DELETE">
  <input type="submit" value="Delete this item" />
</form>
```

See the [crate documentation](https://docs.rs/actix-web-query-method-middleware/latest/actix_web_query_method_middleware/) for details.

## Development

If you have any suggestions or find any bugs, feel free to open a bug report. If
you'd like to contribute, you can send a pull request. If you are thinking of
making a big change, you should open an issue first to discuss the changes to
avoid wasted effort.

### Testing

Run `cargo test` to test things. If you want to see the debug log output, you
can also use `RUST_LOG=debug cargo test` to see debug logs for failed tests.
