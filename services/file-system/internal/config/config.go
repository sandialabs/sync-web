package config

import (
	"fmt"
	"net/url"
	"os"
	"strconv"
)

const defaultMaxObjectBytes = 1024 * 1024

type Config struct {
	Address        string
	GatewayBaseURL string
	MaxObjectBytes int64
}

func Load() (Config, error) {
	cfg := Config{
		Address:        env("SYNC_FS_ADDRESS", ":8080"),
		GatewayBaseURL: env("SYNC_FS_GATEWAY_BASE_URL", "http://gateway/api/v1"),
		MaxObjectBytes: defaultMaxObjectBytes,
	}
	if raw := os.Getenv("SYNC_FS_MAX_OBJECT_BYTES"); raw != "" {
		value, err := strconv.ParseInt(raw, 10, 64)
		if err != nil || value <= 0 {
			return Config{}, fmt.Errorf("SYNC_FS_MAX_OBJECT_BYTES must be a positive integer")
		}
		cfg.MaxObjectBytes = value
	}
	if _, err := url.ParseRequestURI(cfg.GatewayBaseURL); err != nil {
		return Config{}, fmt.Errorf("SYNC_FS_GATEWAY_BASE_URL is invalid: %w", err)
	}
	return cfg, nil
}

func env(name, fallback string) string {
	if value := os.Getenv(name); value != "" {
		return value
	}
	return fallback
}
