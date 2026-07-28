package main

import (
	"context"
	"fmt"
	"os"
	"os/signal"
	"runtime"
	"syscall"

	"orange.dev/native/dataplane"
)

var version = "unknown"

func main() {
	if err := run(os.Args[1:]); err != nil {
		os.Exit(1)
	}
}

func run(arguments []string) error {
	if len(arguments) == 1 && arguments[0] == "version" {
		output := fmt.Sprintf(
			"sing-box version %s\n\nEnvironment: %s %s/%s\nTags: with_quic\nCGO: disabled\n",
			version,
			runtime.Version(),
			runtime.GOOS,
			runtime.GOARCH,
		)
		_, err := os.Stdout.WriteString(output)
		return err
	}
	if len(arguments) != 3 || arguments[1] != "-c" || arguments[2] == "" {
		return fmt.Errorf("invalid arguments")
	}
	switch arguments[0] {
	case "check":
		return dataplane.Check(arguments[2])
	case "run":
		ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
		defer stop()
		go func() {
			<-ctx.Done()
			_ = os.Stdin.Close()
		}()
		runtime, err := dataplane.Start(arguments[2])
		if err != nil {
			return err
		}
		defer runtime.Close()
		return dataplane.NewServer(runtime, os.Stdin, os.Stdout).Run(ctx)
	default:
		return fmt.Errorf("invalid command")
	}
}
