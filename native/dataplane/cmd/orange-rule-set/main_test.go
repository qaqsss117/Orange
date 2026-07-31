package main

import (
	"bytes"
	"os"
	"path/filepath"
	"testing"

	"github.com/sagernet/sing-box/common/srs"
	"github.com/sagernet/sing-box/option"
	singJSON "github.com/sagernet/sing/common/json"
)

const validSource = `{
  "version": 2,
  "rules": [{"domain_suffix": ["example.invalid"]}]
}`

func TestCompileIsDeterministicAndReadableByPinnedSingBox(t *testing.T) {
	directory := t.TempDir()
	source := filepath.Join(directory, "source.json")
	first := filepath.Join(directory, "first.srs")
	second := filepath.Join(directory, "second.srs")
	if err := os.WriteFile(source, []byte(validSource), 0o600); err != nil {
		t.Fatal(err)
	}
	firstMetadata, err := compileRuleSet(source, first)
	if err != nil {
		t.Fatal(err)
	}
	secondMetadata, err := compileRuleSet(source, second)
	if err != nil {
		t.Fatal(err)
	}
	if firstMetadata != secondMetadata || firstMetadata.Version != 2 || firstMetadata.RuleCount != 1 {
		t.Fatalf("unexpected metadata: %#v %#v", firstMetadata, secondMetadata)
	}
	firstContent, err := os.ReadFile(first)
	if err != nil {
		t.Fatal(err)
	}
	secondContent, err := os.ReadFile(second)
	if err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(firstContent, secondContent) {
		t.Fatal("same source produced different SRS bytes")
	}
}

func TestUpgradeV1IsDeterministicAndReadableByPinnedSingBox(t *testing.T) {
	directory := t.TempDir()
	source := filepath.Join(directory, "source-v1.srs")
	first := filepath.Join(directory, "first-v2.srs")
	second := filepath.Join(directory, "second-v2.srs")
	writeLegacyRuleSet(t, source)

	firstMetadata, err := upgradeRuleSet(source, first)
	if err != nil {
		t.Fatal(err)
	}
	secondMetadata, err := upgradeRuleSet(source, second)
	if err != nil {
		t.Fatal(err)
	}
	if firstMetadata != secondMetadata || firstMetadata.Version != 2 || firstMetadata.RuleCount != 1 {
		t.Fatalf("unexpected metadata: %#v %#v", firstMetadata, secondMetadata)
	}
	firstContent, err := os.ReadFile(first)
	if err != nil {
		t.Fatal(err)
	}
	secondContent, err := os.ReadFile(second)
	if err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(firstContent, secondContent) {
		t.Fatal("same legacy source produced different SRS v2 bytes")
	}
}

func TestUpgradeRejectsV2CorruptionAndExistingOutput(t *testing.T) {
	directory := t.TempDir()
	legacy := filepath.Join(directory, "legacy.srs")
	existing := filepath.Join(directory, "existing.srs")
	writeLegacyRuleSet(t, legacy)

	if _, err := upgradeRuleSet(legacy, legacy); err == nil {
		t.Fatal("legacy source overwrite was accepted")
	}
	if err := os.WriteFile(existing, []byte("owned"), 0o600); err != nil {
		t.Fatal(err)
	}
	if _, err := upgradeRuleSet(legacy, existing); err == nil {
		t.Fatal("existing output overwrite was accepted")
	}
	content, err := os.ReadFile(existing)
	if err != nil || string(content) != "owned" {
		t.Fatal("existing output was changed")
	}

	v2 := filepath.Join(directory, "v2.srs")
	source := filepath.Join(directory, "source.json")
	if err := os.WriteFile(source, []byte(validSource), 0o600); err != nil {
		t.Fatal(err)
	}
	if _, err := compileRuleSet(source, v2); err != nil {
		t.Fatal(err)
	}
	if _, err := upgradeRuleSet(v2, filepath.Join(directory, "v2-output.srs")); err == nil {
		t.Fatal("SRS v2 input was accepted by the legacy upgrader")
	}

	corrupt := filepath.Join(directory, "corrupt.srs")
	if err := os.WriteFile(corrupt, []byte("SRS\x01corrupt"), 0o600); err != nil {
		t.Fatal(err)
	}
	if _, err := upgradeRuleSet(corrupt, filepath.Join(directory, "corrupt-output.srs")); err == nil {
		t.Fatal("corrupt legacy SRS was accepted")
	}
}

func TestCompileRejectsOpenEmptyAndUnsupportedSources(t *testing.T) {
	for name, content := range map[string]string{
		"unknown-field": `{"version":2,"rules":[{"domain_suffix":["example.invalid"],"unknown":true}]}`,
		"empty":         `{"version":2,"rules":[]}`,
		"future":        `{"version":4,"rules":[{"domain_suffix":["example.invalid"]}]}`,
	} {
		t.Run(name, func(t *testing.T) {
			directory := t.TempDir()
			source := filepath.Join(directory, "source.json")
			if err := os.WriteFile(source, []byte(content), 0o600); err != nil {
				t.Fatal(err)
			}
			if _, err := compileRuleSet(source, filepath.Join(directory, "output.srs")); err == nil {
				t.Fatal("invalid source was accepted")
			}
		})
	}
}

func TestInspectRejectsCorruptionAndCLIIsClosed(t *testing.T) {
	path := filepath.Join(t.TempDir(), "corrupt.srs")
	if err := os.WriteFile(path, []byte("not-an-srs"), 0o600); err != nil {
		t.Fatal(err)
	}
	if _, err := inspectRuleSet(path); err == nil {
		t.Fatal("corrupt SRS was accepted")
	}
	if err := run([]string{"compile", "arbitrary"}, &bytes.Buffer{}); err == nil {
		t.Fatal("open command arguments were accepted")
	}
}

func TestCompileDoesNotOverwriteSourceOrExistingOutput(t *testing.T) {
	directory := t.TempDir()
	source := filepath.Join(directory, "source.json")
	output := filepath.Join(directory, "output.srs")
	if err := os.WriteFile(source, []byte(validSource), 0o600); err != nil {
		t.Fatal(err)
	}
	if _, err := compileRuleSet(source, source); err == nil {
		t.Fatal("source overwrite was accepted")
	}
	if err := os.WriteFile(output, []byte("owned"), 0o600); err != nil {
		t.Fatal(err)
	}
	if _, err := compileRuleSet(source, output); err == nil {
		t.Fatal("existing output overwrite was accepted")
	}
	content, err := os.ReadFile(output)
	if err != nil || string(content) != "owned" {
		t.Fatal("existing output was changed")
	}
}

func writeLegacyRuleSet(t *testing.T, path string) {
	t.Helper()
	plainRuleSet, err := singJSON.UnmarshalExtended[option.PlainRuleSetCompat]([]byte(validSource))
	if err != nil {
		t.Fatal(err)
	}
	file, err := os.OpenFile(path, os.O_WRONLY|os.O_CREATE|os.O_EXCL, 0o600)
	if err != nil {
		t.Fatal(err)
	}
	if err := srs.Write(file, plainRuleSet.Options, 1); err != nil {
		_ = file.Close()
		t.Fatal(err)
	}
	if err := file.Close(); err != nil {
		t.Fatal(err)
	}
}
