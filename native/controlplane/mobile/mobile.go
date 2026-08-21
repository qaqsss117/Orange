package mobile

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"io"

	"orange.dev/native/controlplane"
)

type Client struct {
	bridge *controlplane.Bridge
}

func NewClient(configJSON []byte) (*Client, error) {
	defer clear(configJSON)
	var config controlplane.Config
	if err := decodeStrict(configJSON, &config); err != nil {
		return nil, errors.New(string(controlplane.ErrorInvalidConfig))
	}
	bridge, err := controlplane.New(context.Background(), config)
	for index := range config.Outbounds {
		config.Outbounds[index].Credential = ""
	}
	if err != nil {
		return nil, publicError(err)
	}
	return &Client{bridge: bridge}, nil
}

func (c *Client) Execute(requestJSON []byte) ([]byte, error) {
	defer clear(requestJSON)
	if c == nil || c.bridge == nil {
		return nil, errors.New(string(controlplane.ErrorClosed))
	}
	var request controlplane.Request
	if err := decodeStrict(requestJSON, &request); err != nil {
		return nil, errors.New(string(controlplane.ErrorInvalidRequest))
	}
	response, err := c.bridge.Execute(context.Background(), request)
	clear(request.Body)
	clear(request.AccessToken)
	if err != nil {
		return nil, publicError(err)
	}
	encoded, err := json.Marshal(response)
	clear(response.Body)
	if err != nil {
		return nil, errors.New(string(controlplane.ErrorUnavailable))
	}
	return encoded, nil
}

func (c *Client) Close() error {
	if c == nil || c.bridge == nil {
		return nil
	}
	bridge := c.bridge
	c.bridge = nil
	if err := bridge.Close(); err != nil {
		return errors.New(string(controlplane.ErrorUnavailable))
	}
	return nil
}

func decodeStrict(content []byte, target any) error {
	decoder := json.NewDecoder(bytes.NewReader(content))
	decoder.DisallowUnknownFields()
	if err := decoder.Decode(target); err != nil {
		return err
	}
	var trailing struct{}
	if err := decoder.Decode(&trailing); !errors.Is(err, io.EOF) {
		return errors.New("trailing JSON")
	}
	return nil
}

func publicError(err error) error {
	var bridgeError *controlplane.BridgeError
	if errors.As(err, &bridgeError) {
		return errors.New(string(bridgeError.Code))
	}
	return errors.New(string(controlplane.ErrorUnavailable))
}
