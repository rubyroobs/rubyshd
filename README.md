# rubyshd

An experimental TLS server written in Rust trying to be agnostic to both the HTTPS and [Gemini](https://geminiprotocol.net/) protocols, and with lots of love for mutual TLS authentication.

I built this mainly to learn more about Gemini and play around in Rust more, but also to host [ruby.sh](https://ruby.sh/).

## Usage

### Running the server

If you ran all the required commands in the "Initial setup" section below in the repository root, the server should start with just:

```shell
cargo run
```

However, there will be no content to serve. You might want to make an `index.html.hbs` in the `public_root` folder in the repository.

To get detailed debug messages about what's going on, run with `RUST_LOG=DEBUG`:

```shell
RUST_LOG=DEBUG cargo run
```

### Folder structure and configuration

`rubyshd` uses 4 folders and 3 files for serving content which are configurable with these environment variables:

- `PUBLIC_ROOT_PATH` - Acts as the public root from which files are served. Defaults to the `public_root` folder in the repository root.
- `ERRDOCS_PATH` - Stores files to be used for error pages (only used for HTTPS as Gemini has no such concept). See the error status code slugs in `src/response.rs` for the possible filenames (i.e. `not_found.html.hbs`) Defaults to the `errdocs` folder in the repository root.
- `PARTIALS_PATH` - Stores Handlebars template partials which can be referenced by other partials and Handlebar template files in the `PUBLIC_ROOT_PATH` or `ERRDOCS_PATH`. Files without the `hbs` extension are ignored. Defaults to the `partials` folder in the repository root.
- `DATA_PATH` - Stores JSON files which are loaded and available under the `data` variable when Handlebars template files are rendered. Files without the `json` extension are ignored. Defaults to the `data` folder in the repository root.
- `TLS_CLIENT_CA_CERTIFICATE_PEM_FILENAME` - A file with PEM-formatted certificate used to verify client certificates during mutual TLS authentication. Defaults to the `ca.cert.pem` file in the repository root.
- `TLS_SERVER_CERTIFICATE_PEM_FILENAME` - A PEM-formatted certificate used for the server. Defaults to the `localhost.cert.pem` file in the repository root.
- `TLS_SERVER_PRIVATE_KEY_PEM_FILENAME` - A PEM-formatted key used for the server. Defaults to the `localhost.pem` file in the repository root.

When running on OpenBSD, the application will lock filesystem access down to just these with [`unveil(2)`](https://man.openbsd.org/unveil.2).

These other configuration options are also configurable by environment variable:

- `MAX_REQUEST_HEADER_SIZE` - The maximum acceptable size for a request. Defaults to 2048.
- `TLS_LISTEN_PORT` - The TLS port to listen on. Both HTTPS and Gemini will be served from this single port - consider using [`relayd(8)`](https://man.openbsd.org/relayd.8) or similar if you want to serve on both 443/1965. Defaults to 4443.
- `DEFAULT_HOSTNAME` - The default hostname used to generate a [`url::Url`](https://docs.rs/url/latest/url/struct.Url.html) when a `Host` header is not present in an HTTPS request. Defaults to `ruby.sh`.

### Routing

The below flow is provided as a reference for how `rubyshd` routes requests, as this works rather differently than other web/Gemini servers. `rubyshd` will use the first file it can successfully load for the response.

- User makes a request to `/path`
- If `{PUBLIC_ROOT_PATH}/path` is a directory...
  - Try `{PUBLIC_ROOT_PATH}/path/index.hbs`
  - If request is HTTPS protocol...
    - Try `{PUBLIC_ROOT_PATH}/path/index.htm`
    - Try `{PUBLIC_ROOT_PATH}/path/index.htm.hbs`
    - Try `{PUBLIC_ROOT_PATH}/path/index.html`
    - Try `{PUBLIC_ROOT_PATH}/path/index.html.hbs`
  - If request is Gemini protocol...
    - Try `{PUBLIC_ROOT_PATH}/path/index.gmi`
    - Try `{PUBLIC_ROOT_PATH}/path/index.gmi.hbs`
- Else...
  - Try `{PUBLIC_ROOT_PATH}/path`
  - Try `{PUBLIC_ROOT_PATH}/path.hbs`
  - If request is HTTPS protocol...
    - Try `{PUBLIC_ROOT_PATH}/path.htm`
    - Try `{PUBLIC_ROOT_PATH}/path.htm.hbs`
    - Try `{PUBLIC_ROOT_PATH}/path.html`
    - Try `{PUBLIC_ROOT_PATH}/path.html.hbs`
  - If request is Gemini protocol...
    - Try `{PUBLIC_ROOT_PATH}/path.gmi`
    - Try `{PUBLIC_ROOT_PATH}/path.gmi.hbs`
  - Try `{PUBLIC_ROOT_PATH}/path.md`
  - Try `{PUBLIC_ROOT_PATH}/path.md.hbs`

All HTTPS responses for static files (i.e. everything except rendered templates/redirects/errors) are marked as cacheable with the `max-age` value set to `CACHEABLE_MAX_AGE_SECONDS`.

### Templates

The [`handlebars-rust`](https://github.com/sunng87/handlebars-rust) project is used for templating and the original [handlebarsjs.com](https://handlebarsjs.com/) documentation is a sufficient reference. However, these `rubyshd`-specific decorators/helpers/quirks are useful to know. Unless otherwise stated, this applies to requests from both the HTTPS and Gemini protocols.

* Only files ending in `.hbs` are treated as templates.
* Files ending in `.md.hbs` are rendered as handlebars templates, converted from Markdown to HTML/Gemtext if necessary, and then rendered again as a template through Handlebars.
* All `.hbs` files in `PARTIALS_PATH` can be loaded in any Handlebars template using the filename without the `.hbs` extension. For example, `{PARTIALS_PATH}/layout.html.hbs` can be used with `{{#> layout.html}}` or similar.
* All `.json` files in `DATA_PATH` are automatically loaded and made available under the `data` property using the filename without the `.json` extension. For example, `{DATA_PATH}/navbar.json` can be used with `{{#each data.navbar}}...{{/each}}` or similar.
* The `*set` decorator can be used to set properties on the context. A key and value must be provided For example, `{{*set "mykey" "a value"}}` will let you then call `{{mykey}}` later in the rendering.
* The `*status` decorator can be used to set the status code used for the response. The value in the last call to the decorator will be the one used. The parameter must be one of the `Status` slugs in `src/response.rs`. For example, `{{*status "unauthenticated"}}` and `{{*status "other_server_error"}}` are valid calls.
* The `*media-type` decorator can be used to set the response media type (i.e. `Content-Type` in HTTPS responses). For example, `{{*media-type "text/csv"}}` and `{{*media-type "application/json"}}` are valid calls. 
* The `*temporary-redirect` and `*permanent-redirect` decorators can be used to set temporary and permanent redirects respectively. For example, `{{*temporary-redirect "https://google.com/"}}` will return a temporary redirect to `https://google.com`. For consistency with Gemini, no response body will be returned with HTTPS responses when a redirect is made regardless of it's position in the template (templates will always render in full unless an error occurs).
* The `pick-random` helper takes an array and chooses a random value from it. For example, if `random_photos.json` contains an array of random photo URLs, `pick-random data.random_photos` will return one of the values from the array.
* The `partial-for-markup` helper takes a name and returns the markup-dependent partial name. For example, `{{partial-for-markup "header"}}` will return `header.gmi` on Gemini protocol requests.
* The following request-specific properties are also available:
  * `peer_addr` - client IP address
  * `path` - the requested path
  * `common_name` - the common name of the client if they authenticated successfully with a client certificate, otherwise `anonymous`
  * `protocol` - the protocol name (`Gemini` or `HTTPS`)
  * `is_authenticated` - if the request was authenticated successfully by mutual TLS with a client certificate
  * `is_anonymous` - opposite of `is_authenticated`
  * `is_https` - if the request was made with HTTPS protocol
  * `is_gemini` - if the request was made with Gemini protocol
  * `os_platform` - the OS platform the server is running on (see [`std::env::consts::OS`](https://doc.rust-lang.org/std/env/consts/constant.OS.html) for a list of possible values)

An example template combining some of these decorators and properties might look like:

```handlebars
{{#if is_anonymous}}{{*status "unauthenticated"}}{{/if}}

{{#if is_gemini}}
  # auth required

  {{#if is_authenticated}}thank you for auth {{common_name}}!{{/if}}
{{else}}
  <h1>auth required!</h1>

  {{#if is_authenticated}}<p>thank you for auth {{common_name}}!</p>{{/if}}
{{/if}}
```

## Initial setup

### Generating a root CA/certificates

```shell
openssl genpkey -algorithm ed25519 -out ca.pem
openssl pkey -in ca.pem -pubout -out ca.pub.pem
openssl req -x509 -sha256 -new -nodes -key ca.pem -days 9999 -out ca.cert.pem
```

### Generating client certificates for our CA

This is only necessary if you want to test with the TLS mutual client authentication.

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

### Generate localhost self-signed server certs/keys

```shell
openssl req -x509 -newkey rsa:4096 -sha256  -days 3650 \
  -nodes -keyout localhost.pem -out localhost.cert.pem -subj "/CN=localhost" \
  -addext "subjectAltName=DNS:localhost,IP:127.0.0.1"
```

### (For deploying only) Generate Let's Encrypt certs/keys

This uses CloudFlare DNS verification and generates a wildcard certificate.

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

## Future work

- Better tests and CI
- Macro or similar for quickly creating handlebars helpers
- Overall code cleanup
