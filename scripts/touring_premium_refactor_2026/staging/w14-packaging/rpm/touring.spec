# Touring RPM spec
Name:           touring
Version:        %{?_version}
Release:        1%{?dist}
Summary:        Code intelligence and AI-assisted refactoring

License:        MIT OR Apache-2.0
URL:            https://touring.dev
Source0:        %{name}-%{version}.tar.gz

BuildRequires:  rust >= 1.83
Requires:       glibc

%description
Touring is a code intelligence platform providing AST-based blast radius
analysis, BM25 symbol search, and reinforcement-learning-guided refactoring.

%prep
%autosetup

%build
cargo build --release --frozen

%install
mkdir -p %{buildroot}/usr/lib/touring/bin
install -m 0755 target/release/touring %{buildroot}/usr/lib/touring/bin/
mkdir -p %{buildroot}/usr/local/bin
ln -s /usr/lib/touring/bin/touring %{buildroot}/usr/local/bin/touring

%files
/usr/lib/touring/bin/touring
/usr/local/bin/touring

%changelog
* %{?_changedate} Touring Team - %{version}-1
- Release %{version}
