package main

import (
	"bytes"
	"os"
	"path/filepath"
	"testing"
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
