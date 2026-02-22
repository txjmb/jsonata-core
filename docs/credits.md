# Credits

jsonatapy stands on the shoulders of giants. This page acknowledges the projects, people, and communities that made this work possible.

## JSONata Project

### Original JSONata

**Created by:** Andrew Coleman

The [JSONata project](https://jsonata.org/) is the foundational work that defined the JSONata query and transformation language. Andrew Coleman's vision of a lightweight, functional language for JSON processing has enabled countless data transformation use cases.

- **Website:** [jsonata.org](https://jsonata.org/)
- **Documentation:** [docs.jsonata.org](https://docs.jsonata.org/)
- **Interactive Playground:** [try.jsonata.org](https://try.jsonata.org/)

### JavaScript Reference Implementation

**Repository:** [jsonata-js](https://github.com/jsonata-js/jsonata)

The JavaScript reference implementation (jsonata-js) serves as the authoritative specification for JSONata behavior. jsonatapy mirrors this implementation to ensure 100% compatibility.

**License:** MIT

## jsonatapy Contributors

### Core Team

Thank you to all contributors who have helped build jsonatapy:

- Contributors list will be maintained as the project grows
- See [GitHub Contributors](https://github.com/txjmb/jsonata-core/graphs/contributors) for the complete list

### Community

Special thanks to:
- Early adopters who provided feedback
- Beta testers who helped find and fix issues
- Documentation contributors
- Bug reporters and issue triagers

## Technology Stack

### Rust Programming Language

jsonatapy is built with [Rust](https://www.rust-lang.org/), providing:
- Memory safety without garbage collection
- Zero-cost abstractions
- Fearless concurrency
- Excellent performance

**License:** Apache 2.0 / MIT

### PyO3

[PyO3](https://pyo3.rs/) enables seamless Rust-Python interoperability:
- Native Python extension modules from Rust
- Zero-overhead Python API
- Automatic type conversions
- Excellent tooling and documentation

**Repository:** [github.com/PyO3/pyo3](https://github.com/PyO3/pyo3)

**License:** Apache 2.0 / MIT

### Maturin

[Maturin](https://maturin.rs/) simplifies building and publishing Python packages written in Rust:
- Easy build configuration
- Multi-platform wheel building
- PyPI publishing integration
- Development mode support

**Repository:** [github.com/PyO3/maturin](https://github.com/PyO3/maturin)

**License:** Apache 2.0 / MIT

## Open Source Dependencies

jsonatapy relies on excellent open-source libraries:

### Rust Crates

- **serde & serde_json** - Serialization framework
- **indexmap** - Hash map with insertion order
- **regex** - Regular expression engine
- **thiserror** - Error handling macros

See [Cargo.toml](../Cargo.toml) for the complete dependency list.

### Python Packages

- **pytest** - Testing framework
- **black** - Code formatter
- **ruff** - Linter
- **mypy** - Type checker

## Inspiration

### Similar Projects

jsonatapy was inspired by:

- **jsonata-python** - First Python wrapper for JSONata (via JavaScript engine)
- **Other language implementations** - Go, Java, .NET ports of JSONata
- **jq** - Command-line JSON processor that inspired JSONata

### Rust-Python Ecosystem

Examples and patterns from the broader Rust-Python community:
- **pydantic-core** - High-performance validation library
- **polars** - Fast DataFrame library
- **ruff** - Extremely fast Python linter
- **orjson** - Fast JSON library

These projects demonstrated the potential of Rust for Python extensions.

## JSONata Community

Thanks to the broader JSONata community:

- **Forum participants** who answer questions
- **Blog authors** who write tutorials
- **Conference speakers** who promote JSONata
- **Enterprise users** who provide real-world feedback

## Testing

### Reference Test Suite

jsonatapy achieves 100% compatibility by passing all 1258 tests from the jsonata-js reference test suite. This comprehensive test coverage is thanks to the JSONata maintainers' commitment to quality.

**Location:** [jsonata-js/test/test-suite](https://github.com/jsonata-js/jsonata/tree/master/test/test-suite)

## Documentation

### Resources

Documentation references and inspiration:
- [JSONata Language Reference](https://docs.jsonata.org/)
- [Rust Book](https://doc.rust-lang.org/book/)
- [PyO3 Guide](https://pyo3.rs/latest/)
- [Python Packaging Guide](https://packaging.python.org/)

## Special Thanks

### Development Tools

- **GitHub** - Repository hosting and CI/CD
- **PyPI** - Python package distribution
- **Docs.rs** - Rust documentation hosting
- **crates.io** - Rust package registry

### IDEs and Editors

- **VS Code** with rust-analyzer
- **PyCharm** for Python development
- **Vim/Neovim** with LSP support

## Recognition

### Performance Achievements

jsonatapy achieves **4.5x faster than JavaScript** on average, thanks to:
- Rust's zero-cost abstractions
- Efficient memory management (Rc-wrapped values)
- Optimized evaluation strategies
- Native compiled code vs interpreted JavaScript

This performance enables new use cases and higher throughput applications.

## How to Contribute

Want to be part of the credits? See our [Contributing Guide](development/contributing.md) to get started!

### Ways to Contribute

- **Code contributions** - Bug fixes, new features, optimizations
- **Documentation** - Tutorials, examples, API docs
- **Testing** - Bug reports, test cases, performance testing
- **Community** - Help others, write blog posts, give talks

## Contact

- **Issues:** [GitHub Issues](https://github.com/txjmb/jsonata-core/issues)
- **Discussions:** [GitHub Discussions](https://github.com/txjmb/jsonata-core/discussions)
- **Email:** Contact information (if available)

## License

jsonatapy is released under the MIT License, the same license as the JSONata reference implementation. This ensures maximum compatibility and ease of adoption.

See [License](license.md) for full details.

---

**Thank you to everyone who has contributed to making jsonatapy possible!**

If we've missed anyone, please open a PR to update this page.
