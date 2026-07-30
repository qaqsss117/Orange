package main

import (
	"bytes"
	"encoding/json"
	"errors"
	"io"
	"os"
	"path/filepath"

	"github.com/sagernet/sing-box/common/srs"
	"github.com/sagernet/sing-box/option"
	singJSON "github.com/sagernet/sing/common/json"
)

const (
	supportedRuleSetVersion = 2
	maximumSourceBytes      = 4 * 1024 * 1024
	maximumBinaryBytes      = 16 * 1024 * 1024
)

type ruleSetMetadata struct {
	Version   uint8 `json:"version"`
	RuleCount int   `json:"rule_count"`
}

func main() {
	if err := run(os.Args[1:], os.Stdout); err != nil {
		os.Exit(1)
	}
}

func run(arguments []string, output io.Writer) error {
	var (
		metadata ruleSetMetadata
		err      error
	)
	switch {
	case len(arguments) == 5 && arguments[0] == "compile" && arguments[1] == "--source" && arguments[3] == "--output":
		metadata, err = compileRuleSet(arguments[2], arguments[4])
	case len(arguments) == 3 && arguments[0] == "inspect" && arguments[1] == "--input":
		metadata, err = inspectRuleSet(arguments[2])
	default:
		return errors.New("invalid arguments")
	}
	if err != nil {
		return err
	}
	return json.NewEncoder(output).Encode(metadata)
}

func compileRuleSet(sourcePath string, outputPath string) (ruleSetMetadata, error) {
	if sourcePath == "" || outputPath == "" {
		return ruleSetMetadata{}, errors.New("missing path")
	}
	sourceAbsolute, err := filepath.Abs(sourcePath)
	if err != nil {
		return ruleSetMetadata{}, errors.New("invalid source path")
	}
	outputAbsolute, err := filepath.Abs(outputPath)
	if err != nil || sourceAbsolute == outputAbsolute {
		return ruleSetMetadata{}, errors.New("invalid output path")
	}
	content, err := readRegularFile(sourceAbsolute, maximumSourceBytes)
	if err != nil {
		return ruleSetMetadata{}, err
	}
	plainRuleSet, err := singJSON.UnmarshalExtended[option.PlainRuleSetCompat](content)
	if err != nil || plainRuleSet.Version != supportedRuleSetVersion || !validRules(plainRuleSet.Options) {
		return ruleSetMetadata{}, errors.New("invalid rule-set source")
	}
	if _, err := os.Lstat(outputAbsolute); err == nil || !errors.Is(err, os.ErrNotExist) {
		return ruleSetMetadata{}, errors.New("output already exists or is unavailable")
	}
	outputDirectory := filepath.Dir(outputAbsolute)
	temporary, err := os.CreateTemp(outputDirectory, ".orange-rule-set-*.tmp")
	if err != nil {
		return ruleSetMetadata{}, errors.New("cannot create temporary output")
	}
	temporaryPath := temporary.Name()
	committed := false
	defer func() {
		_ = temporary.Close()
		if !committed {
			_ = os.Remove(temporaryPath)
		}
	}()
	if err := srs.Write(temporary, plainRuleSet.Options, supportedRuleSetVersion); err != nil {
		return ruleSetMetadata{}, errors.New("cannot encode rule-set")
	}
	if err := temporary.Sync(); err != nil {
		return ruleSetMetadata{}, errors.New("cannot sync rule-set")
	}
	if err := temporary.Close(); err != nil {
		return ruleSetMetadata{}, errors.New("cannot close rule-set")
	}
	if err := os.Chmod(temporaryPath, 0o644); err != nil {
		return ruleSetMetadata{}, errors.New("cannot remove executable permissions")
	}
	if err := os.Rename(temporaryPath, outputAbsolute); err != nil {
		return ruleSetMetadata{}, errors.New("cannot commit rule-set")
	}
	committed = true
	return inspectRuleSet(outputAbsolute)
}

func inspectRuleSet(path string) (ruleSetMetadata, error) {
	content, err := readRegularFile(path, maximumBinaryBytes)
	if err != nil {
		return ruleSetMetadata{}, err
	}
	plainRuleSet, err := srs.Read(bytes.NewReader(content), false)
	if err != nil || plainRuleSet.Version != supportedRuleSetVersion || !validRules(plainRuleSet.Options) {
		return ruleSetMetadata{}, errors.New("invalid rule-set binary")
	}
	return ruleSetMetadata{
		Version:   plainRuleSet.Version,
		RuleCount: len(plainRuleSet.Options.Rules),
	}, nil
}

func readRegularFile(path string, maximumBytes int64) ([]byte, error) {
	info, err := os.Lstat(path)
	if err != nil || !info.Mode().IsRegular() || info.Mode()&os.ModeSymlink != 0 || info.Size() > maximumBytes {
		return nil, errors.New("file is unavailable or outside bounds")
	}
	return os.ReadFile(path)
}

func validRules(ruleSet option.PlainRuleSet) bool {
	if len(ruleSet.Rules) == 0 {
		return false
	}
	for _, rule := range ruleSet.Rules {
		if !rule.IsValid() {
			return false
		}
	}
	return true
}
