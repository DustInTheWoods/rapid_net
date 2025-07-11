# Socket Tests

This directory contains tests for the RapidNet library, including tests for both TCP and Unix socket functionality.

## TCP Socket Tests

The TCP socket tests are implemented in `socket_tests.rs` and include:

1. **Basic Server Initialization Test**: Tests that a server can be created with a TCP socket.
2. **Client-Server Communication Test**: Tests basic communication between a client and server using TCP sockets.
3. **Multiple Clients Test**: Tests that a server can handle multiple clients simultaneously using TCP sockets.

## Unix Socket Tests

The Unix socket tests are also implemented in `socket_tests.rs` but are conditionally compiled to only run on Unix platforms using the `#[cfg(unix)]` attribute. These tests include:

1. **Basic Server Initialization Test**: Tests that a server can be created with a Unix socket.
2. **Client-Server Communication Test**: Tests basic communication between a client and server using Unix sockets.
3. **Multiple Clients Test**: Tests that a server can handle multiple clients simultaneously using Unix sockets.
4. **Performance Comparison Test**: Compares the performance of TCP and Unix sockets by measuring the time it takes to send and receive large messages.

## Running the Tests

To run the tests, use the following command:

```bash
cargo test --test socket_tests
```

Note that the Unix socket tests will only run on Unix platforms (Linux, macOS, etc.) and will be skipped on Windows.

## Known Issues

### Windows Compatibility

The Unix socket tests are designed to be skipped on Windows since Unix sockets are not supported on Windows. The library code has been conditionally compiled to handle this case gracefully:

1. All Unix socket-specific code is wrapped in `#[cfg(unix)]` attributes.
2. On non-Unix platforms, appropriate error messages are provided when attempting to use Unix sockets.

### Tokio Dependency

There may be issues with the Tokio dependency when running the tests on Windows. This is because the `net-unix` feature of Tokio is not available on Windows. The library has been designed to handle this case by conditionally compiling the Unix socket functionality, but the dependency resolution in Cargo may still cause issues.

If you encounter errors related to the Tokio dependency when running the tests on Windows, you can try the following:

1. Delete the `Cargo.lock` file and let Cargo generate a new one.
2. Modify the `Cargo.toml` file to conditionally include the `net-unix` feature only on Unix platforms.

## Future Improvements

In the future, we plan to add more comprehensive tests for both TCP and Unix socket functionality, including:

1. Error handling tests
2. Reconnection tests
3. Load testing
4. More detailed performance comparisons