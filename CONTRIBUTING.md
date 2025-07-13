# Contributing to the EuroScope plugin

Thank you for your interest in contributing to the BARS EuroScope plugin! This guide will help you get started with contributing.

## Getting Started

These instructions are designed for building on Linux systems.

### Prerequisites

- Standard development environment (eg. `build-essential`)
- Rust
	- Cargo
	- `i686-pc-windows-msvc` target
- Clang
	- MSVC compatibility tools (eg. `clang-cl`)
	- `i686-pc-windows-msvc` target
- Windows SDK (32-bit)
	- Install with [Xwin](https://github.com/Jake-Shadle/xwin/)

### Development Setup

1. **Fork and Clone**

   ```bash
   git clone https://github.com/stopbars/EuroScope-Plugin
   cd EuroScope-Plugin
   ```
   <br>

2. **Configure Environment Variables**

   ```bash
   # set according to the clang version installed
   export XCC=clang-cl-<version>
   export XLD=lld-link-<version>

   # set according to xwin SDK location, such that $XWIN/{crt,sdk}/ exist
   export XWIN=/path/to/xwin/install/dir
   ```

   <br>

3. **Build Plugin**

   ```bash
   make
   ```

## Development Guidelines

### Code Style

- C/++ code shall follow the ClangFormat configuration provided
- Rust code shall follow the Rustfmt configuration provided
- Formatting can be applied to all code by running `make format`

### Project Structure

- client: Rust components of the EuroScope plugin, including interaction with the BARS API
- plugin: C++ components of the EuroScope plugin, including interaction with the EuroScope API
- shared: shared libraries and data
	- config: data structures and parsing for the EuroScope plugin configuration
	- protocol: data structures for the BARS API protocol and EuroScope plugin internal synchronisation formats
- tool: executables supporting development or use of the plugin
	- server: a basic mock of the BARS API server

## Contribution Process

### 1. Find or Create an Issue

- Browse existing issues for bug fixes or feature requests
- Create a new issue for significant changes
- Discuss the approach before starting work

### 2. Create a Feature Branch

```bash
git checkout -b feature/your-feature-name
# or
git checkout -b fix/your-bug-fix
```

### 3. Make Your Changes

- Write clean, well-documented code
- Test your changes thoroughly
- Update documentation if necessary

### 4. Commit Your Changes

```bash
git add .
git commit -m "Add brief description of your changes"
```

Use clear, descriptive commit messages:

- `feat: add support for approach lighting`
- `fix: resolve stopbar state synchronization issue`
- `docs: update contribution documentation`

Commit messages should follow guidelines from [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/). Where appropriate, include a scope in the commit message; see the commit history for extant scopes.

### 5. Push and Create Pull Request

```bash
git push origin feature/your-feature-name
```

Create a pull request with:

- Clear description of changes
- Reference to related issues
- Screenshots for UI changes (if applicable)

## Getting Help

- **Discord**: Join the BARS [Discord](https://stopbars.com/discord) server for real-time help
- **GitHub Issues**: [Create an issue](https://github.com/stopbars/xxx/issues/new) for bugs or feature requests
- **Code Review**: Ask for reviews on complex changes

## Recognition

Contributors are recognized in:

- Release notes for significant contributions
- BARS website credits page (coming soon)
- BARS Discord Role (@Contributer)

Your contributions directly support the ongoing development and improvement of BARS. By getting involved, you help us build a more robust, and feature-rich product that benefits the entire flight sim community.
