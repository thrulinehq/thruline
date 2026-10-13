// Produce cosign-format encrypted private keys for curves/algorithms that are
// NOT P-256, using the SAME code path cosign uses:
//   x509.MarshalPKCS8PrivateKey -> encrypted.Encrypt -> pem.EncodeToMemory
// (cosign/pkg/cosign/keys.go marshalKeyPair).
package main

import (
	"crypto/ecdsa"
	"crypto/ed25519"
	"crypto/elliptic"
	"crypto/rand"
	"crypto/rsa"
	"crypto/x509"
	"encoding/pem"
	"fmt"
	"os"

	"github.com/secure-systems-lab/go-securesystemslib/encrypted"
)

const sigstoreType = "ENCRYPTED SIGSTORE PRIVATE KEY"

func write(name string, priv any, pass []byte) {
	der, err := x509.MarshalPKCS8PrivateKey(priv)
	if err != nil {
		panic(err)
	}
	enc, err := encrypted.Encrypt(der, pass)
	if err != nil {
		panic(err)
	}
	out := pem.EncodeToMemory(&pem.Block{Type: sigstoreType, Bytes: enc})
	if err := os.WriteFile(name, out, 0o600); err != nil {
		panic(err)
	}
	// Prove Go itself reads it back through cosign's own path.
	p, _ := pem.Decode(out)
	back, err := encrypted.Decrypt(p.Bytes, pass)
	if err != nil {
		panic(err)
	}
	k, err := x509.ParsePKCS8PrivateKey(back)
	if err != nil {
		panic(err)
	}
	fmt.Printf("%-28s pkcs8=%4d bytes  Go ParsePKCS8PrivateKey -> %T\n", name, len(back), k)
}

func main() {
	pass := []byte("hello")
	p384, err := ecdsa.GenerateKey(elliptic.P384(), rand.Reader)
	if err != nil {
		panic(err)
	}
	write(os.Args[1]+"/p384-key.pem", p384, pass)

	_, ed, err := ed25519.GenerateKey(rand.Reader)
	if err != nil {
		panic(err)
	}
	write(os.Args[1]+"/ed25519-key.pem", ed, pass)

	rsaKey, err := rsa.GenerateKey(rand.Reader, 2048)
	if err != nil {
		panic(err)
	}
	write(os.Args[1]+"/rsa2048-key.pem", rsaKey, pass)

	// A P-256 key produced by Go through the exact same path, as the control:
	// if this one is refused too, the refusal is not about the curve.
	p256, err := ecdsa.GenerateKey(elliptic.P256(), rand.Reader)
	if err != nil {
		panic(err)
	}
	write(os.Args[1]+"/p256-control-key.pem", p256, pass)
}
