#!/bin/sh
# One-time (macOS): create a self-signed "dust dev" code-signing identity in the
# login keychain for local builds. Not for distribution; releases should be signed
# with an Apple Developer ID and notarized. macOS asks for your password to trust it.
set -e
if security find-identity -v -p codesigning | grep -q '"dust dev"'; then
  echo '"dust dev" identity already exists'; exit 0
fi
dir=$(mktemp -d); trap 'rm -rf "$dir"' EXIT
cat > "$dir/ext.cnf" <<'CNF'
[req]
distinguished_name = dn
prompt = no
[dn]
CN = dust dev
[ext]
basicConstraints = critical,CA:false
keyUsage = critical,digitalSignature
extendedKeyUsage = critical,codeSigning
CNF
openssl req -x509 -newkey rsa:2048 -nodes -keyout "$dir/key.pem" -out "$dir/cert.pem" -days 3650 -config "$dir/ext.cnf" -extensions ext
pass=$(openssl rand -hex 12)
openssl pkcs12 -export -legacy -inkey "$dir/key.pem" -in "$dir/cert.pem" -name "dust dev" -out "$dir/id.p12" -passout "pass:$pass"
security import "$dir/id.p12" -k ~/Library/Keychains/login.keychain-db -P "$pass" -T /usr/bin/codesign
security add-trusted-cert -r trustRoot -p codeSign -k ~/Library/Keychains/login.keychain-db "$dir/cert.pem"
echo 'created "dust dev" signing identity'
