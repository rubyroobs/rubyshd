# rubyshd

## Usage

### Generating a root CA/certificates

```shell
openssl genpkey -algorithm ed25519 -out ca.pem
openssl pkey -in ca.pem -pubout -out ca.pub.pem
openssl req -x509 -sha256 -new -nodes -key ca.pem -days 9999 -out ca.cert.pem
```

### Generating client certificates for our CA

ECC

```shell
openssl req -newkey ed25519 -days 1000 -nodes -keyout client.pem > client.certreq.pem
openssl x509 -req -in client.certreq.pem -days 1000 -CA ca.cert.pem -CAkey ca.pem -set_serial 01 > client.cert.pem
openssl pkcs12 -export -legacy -in client.cert.pem -inkey client.pem -out client.pfx
rm client.certreq.pem
rm client.cert.pem
rm client.pem
```

RSA (macOS etc)

```shell
openssl req -newkey rsa:2048 -days 1000 -nodes -keyout client.pem > client.certreq.pem
openssl x509 -req -in client.certreq.pem -days 1000 -CA ca.cert.pem -CAkey ca.pem -set_serial 01 > client.cert.pem
openssl pkcs12 -export -legacy -in client.cert.pem -inkey client.pem -out client.pfx
rm client.certreq.pem
rm client.cert.pem
rm client.pem
```

### ruby.sh keys

```shell
export CF_Token="TOKEN"
export CF_Email="ruby@ruby.sh"
acme.sh --issue --dns dns_cf -d ruby.sh -d '*.ruby.sh' --server letsencrypt
acme.sh --renew -d ruby.sh -d '*.ruby.sh' --server letsencrypt
cp /Users/ruby/.acme.sh/ruby.sh_ecc/ruby.sh.cer ruby.sh.cert.pem
cp /Users/ruby/.acme.sh/ruby.sh_ecc/ruby.sh.key ruby.sh.pem
cp /Users/ruby/.acme.sh/ruby.sh_ecc/ca.cer ruby.sh.intermediate.pem
cp /Users/ruby/.acme.sh/ruby.sh_ecc/fullchain.cer ruby.sh.fullchain.pem
```