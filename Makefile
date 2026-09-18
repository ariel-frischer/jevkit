
.PHONY: help install i test test-v test-coverage lint lint-go format clean build bin run go-install install-global uninstall release patch minor major prep-release

MODULE_PATH=github.com/demo/jevkit
BUILD_VERSION?=$(shell git tag --sort=-v:refname 2>/dev/null | head -1)
ifeq ($(BUILD_VERSION),)
  BUILD_VERSION=dev
endif
COMMIT=$(shell git rev-parse --short HEAD 2>/dev/null || echo "unknown")
BUILD_DATE=$(shell date -u +%Y-%m-%dT%H:%M:%SZ)
LDFLAGS=-ldflags="-X ${MODULE_PATH}/internal/version.Version=${BUILD_VERSION} \
                   -X ${MODULE_PATH}/internal/version.Commit=${COMMIT} \
                   -X ${MODULE_PATH}/internal/version.BuildDate=${BUILD_DATE} \
                   -s -w"

help: ## Show this help message
	@echo 'Usage: make [target]'
	@echo ''
	@echo 'Available targets:'
	@awk 'BEGIN {FS = ":.*?## "} /^[a-zA-Z_-]+:.*?## / {printf "  %-15s %s\n", $$1, $$2}' $(MAKEFILE_LIST)

install: ## Download dependencies
	go mod download

i: install ## Alias for install

go-install: ## Install jevkit to GOPATH/bin
	go install ${LDFLAGS} ./cmd/jevkit/

install-global: go-install ## Alias for go-install

test: ## Run tests
	go test ./...

test-v: ## Run tests (verbose)
	go test -v ./...

test-coverage: ## Run tests with coverage
	go test -race -coverprofile=coverage.out ./...

lint: lint-go ## Run all linters

lint-go: ## Run Go linters
	@if command -v golangci-lint >/dev/null 2>&1; then \
		golangci-lint run; \
	else \
		echo "golangci-lint not installed, running go vet"; \
		go vet ./...; \
	fi

format: ## Format code
	go fmt ./...

clean: ## Clean build artifacts
	go clean
	rm -rf bin/ coverage.out

build: ## Build binary with version info
	go build ${LDFLAGS} -o bin/jevkit ./cmd/jevkit/

bin: build ## Alias for build

run: ## Run main package
	go run ${LDFLAGS} ./cmd/jevkit/

uninstall: ## Uninstall jevkit
	@./uninstall.sh

##@ Release
prep-release: ## Full release flow (usage: make prep-release VERSION=v0.1.0)
ifndef VERSION
	$(error VERSION is required, e.g. make prep-release VERSION=v1.0.0)
endif
	@./scripts/release.sh $(VERSION)

release: prep-release ## Run release preflight, create tag, and push (usage: make release VERSION=v1.0.0)

patch: ## Bump patch version and release
	$(eval CURRENT=$(shell git tag --sort=-v:refname | head -1 | sed 's/^v//'))
	$(eval NEXT=v$(shell echo $(CURRENT) | awk -F. '{printf "%d.%d.%d", $$1, $$2, $$3+1}'))
	@echo "Bumping $(CURRENT) -> $(NEXT)"
	$(MAKE) release VERSION=$(NEXT)

minor: ## Bump minor version and release
	$(eval CURRENT=$(shell git tag --sort=-v:refname | head -1 | sed 's/^v//'))
	$(eval NEXT=v$(shell echo $(CURRENT) | awk -F. '{printf "%d.%d.0", $$1, $$2+1}'))
	@echo "Bumping $(CURRENT) -> $(NEXT)"
	$(MAKE) release VERSION=$(NEXT)

major: ## Bump major version and release
	$(eval CURRENT=$(shell git tag --sort=-v:refname | head -1 | sed 's/^v//'))
	$(eval NEXT=v$(shell echo $(CURRENT) | awk -F. '{printf "%d.0.0", $$1+1}'))
	@echo "Bumping $(CURRENT) -> $(NEXT)"
	$(MAKE) release VERSION=$(NEXT)
