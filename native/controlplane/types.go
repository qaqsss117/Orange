package controlplane

import (
	"errors"
	"fmt"
	"time"
)

type Protocol string

const (
	ProtocolShadowsocks Protocol = "shadowsocks"
	ProtocolTrojan      Protocol = "trojan"
	ProtocolHysteria2   Protocol = "hysteria2"
	ProtocolVLESS       Protocol = "vless"
)

type OutboundConfig struct {
	Protocol          Protocol
	Server            string
	Port              uint16
	Credential        string
	TLSServerName     string
	ShadowsocksMethod string
	RealityPublicKey  string
	RealityShortID    string
	ClientFingerprint string
	VLESSFlow         string
}

type DNSProtocol string

const (
	DNSProtocolUDP DNSProtocol = "udp"
	DNSProtocolTCP DNSProtocol = "tcp"
	DNSProtocolTLS DNSProtocol = "tls"
)

type StartupDNS struct {
	Protocol      DNSProtocol
	Server        string
	Port          uint16
	TLSServerName string
}

type Limits struct {
	ConnectTimeout   time.Duration
	RequestTimeout   time.Duration
	MaxConcurrent    int
	MaxRequestBytes  int64
	MaxResponseBytes int64
}

func DefaultLimits() Limits {
	return Limits{
		ConnectTimeout:   10 * time.Second,
		RequestTimeout:   30 * time.Second,
		MaxConcurrent:    16,
		MaxRequestBytes:  1 << 20,
		MaxResponseBytes: 4 << 20,
	}
}

type Config struct {
	Outbound     OutboundConfig
	StartupDNS   []StartupDNS
	AllowedHosts []string
	Limits       Limits
}

type Request struct {
	Method      string `json:"method"`
	Host        string `json:"host"`
	Path        string `json:"path"`
	ContentType string `json:"contentType,omitempty"`
	Body        []byte `json:"body,omitempty"`
	AccessToken []byte `json:"accessToken,omitempty"`
}

type Response struct {
	StatusCode  int    `json:"statusCode"`
	ContentType string `json:"contentType,omitempty"`
	Body        []byte `json:"body"`
}

type ErrorCode string

const (
	ErrorInvalidConfig    ErrorCode = "invalid-config"
	ErrorInvalidRequest   ErrorCode = "invalid-request"
	ErrorClosed           ErrorCode = "closed"
	ErrorUnavailable      ErrorCode = "bootstrap-unavailable"
	ErrorTimeout          ErrorCode = "timeout"
	ErrorCanceled         ErrorCode = "canceled"
	ErrorDNS              ErrorCode = "dns-failure"
	ErrorTLS              ErrorCode = "tls-failure"
	ErrorResponseTooLarge ErrorCode = "response-too-large"
)

type BridgeError struct {
	Code  ErrorCode
	cause error
}

func (e *BridgeError) Error() string {
	return string(e.Code)
}

func (e *BridgeError) Unwrap() error {
	return e.cause
}

func errorWithCode(code ErrorCode, cause error) error {
	return &BridgeError{Code: code, cause: cause}
}

func IsErrorCode(err error, code ErrorCode) bool {
	var bridgeError *BridgeError
	return errors.As(err, &bridgeError) && bridgeError.Code == code
}

func invalidConfig(name string) error {
	return errorWithCode(ErrorInvalidConfig, fmt.Errorf("invalid %s", name))
}
