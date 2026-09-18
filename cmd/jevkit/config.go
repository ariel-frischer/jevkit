package main

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
	"time"

	"github.com/demo/jevkit/internal/config"
	"github.com/spf13/cobra"
	"gopkg.in/yaml.v3"
)

var configForce bool

var configCmd = &cobra.Command{
	Use:   "config",
	Short: "Manage user-level configuration",
}

var configInitCmd = &cobra.Command{
	Use:   "init",
	Short: "Create a default config file",
	RunE:  runConfigInit,
}

var configShowCmd = &cobra.Command{
	Use:   "show",
	Short: "Show current configuration",
	RunE:  runConfigShow,
}

var configPathCmd = &cobra.Command{
	Use:   "path",
	Short: "Print config file path",
	RunE:  runConfigPath,
}

var configEditCmd = &cobra.Command{
	Use:   "edit",
	Short: "Open config in $EDITOR",
	RunE:  runConfigEdit,
}

var configSetCmd = &cobra.Command{
	Use:   "set <key> <value>",
	Short: "Set a user-level configuration value",
	Args:  cobra.ExactArgs(2),
	RunE:  runConfigSet,
}

var configGetCmd = &cobra.Command{
	Use:   "get <key>",
	Short: "Get a user-level configuration value",
	Args:  cobra.ExactArgs(1),
	RunE:  runConfigGet,
}

var configToggleCmd = &cobra.Command{
	Use:   "toggle <key>",
	Short: "Toggle a boolean user-level configuration value",
	Args:  cobra.ExactArgs(1),
	RunE:  runConfigToggle,
}

var configKeysCmd = &cobra.Command{
	Use:   "keys",
	Short: "List configurable keys",
	RunE:  runConfigKeys,
}

func init() {
	configInitCmd.Flags().BoolVar(&configForce, "force", false, "overwrite existing config")
	configCmd.AddCommand(configInitCmd)
	configCmd.AddCommand(configShowCmd)
	configCmd.AddCommand(configPathCmd)
	configCmd.AddCommand(configEditCmd)
	configCmd.AddCommand(configSetCmd)
	configCmd.AddCommand(configGetCmd)
	configCmd.AddCommand(configToggleCmd)
	configCmd.AddCommand(configKeysCmd)
}

func runConfigInit(cmd *cobra.Command, args []string) error {
	path := selectedConfigPath()
	if _, err := os.Stat(path); err == nil && !configForce {
		return fmt.Errorf("%s already exists (use --force to overwrite)", path)
	}

	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return fmt.Errorf("creating config directory: %w", err)
	}

	content := `# jevkit configuration
# All fields are optional. CLI flags always override these values.

# Example output format for commands that support formatted output.
# output_format: text

# Disable colored output by default.
# no_color: false

# Example nested setting.
# display:
#   theme: auto
`
	if err := os.WriteFile(path, []byte(content), 0o644); err != nil {
		return fmt.Errorf("writing %s: %w", path, err)
	}
	_, err := fmt.Fprintf(cmd.OutOrStdout(), "Created %s\n", path)
	return err
}

func runConfigPath(cmd *cobra.Command, args []string) error {
	_, err := fmt.Fprintln(cmd.OutOrStdout(), selectedConfigPath())
	return err
}

func runConfigShow(cmd *cobra.Command, args []string) error {
	cfg, err := config.Load(selectedConfigPath())
	if err != nil {
		return err
	}

	out, err := yaml.Marshal(cfg)
	if err != nil {
		return fmt.Errorf("marshaling config: %w", err)
	}
	_, err = fmt.Fprint(cmd.OutOrStdout(), string(out))
	return err
}

func runConfigEdit(cmd *cobra.Command, args []string) error {
	path := selectedConfigPath()
	if _, err := os.Stat(path); os.IsNotExist(err) {
		fmt.Fprintf(os.Stderr, "warning: %s not found - run `jevkit config init` first\n", path)
	}

	editor := os.Getenv("EDITOR")
	if editor == "" {
		editor = "vi"
	}

	c := exec.Command(editor, path)
	c.Stdin = os.Stdin
	c.Stdout = os.Stdout
	c.Stderr = os.Stderr
	return c.Run()
}

type configValueType string

const (
	configTypeString   configValueType = "string"
	configTypeBool     configValueType = "bool"
	configTypeInt      configValueType = "int"
	configTypeDuration configValueType = "duration"
)

type configKeySpec struct {
	Key         string
	Type        configValueType
	Description string
	Validate    func(string) error
}

var configKeySpecs = []configKeySpec{
	{Key: "output_format", Type: configTypeString, Description: "Default output format", Validate: oneOf("text", "json", "yaml")},
	{Key: "no_color", Type: configTypeBool, Description: "Disable colored output"},
	{Key: "display.theme", Type: configTypeString, Description: "Display theme", Validate: oneOf("auto", "light", "dark")},
}

func runConfigSet(cmd *cobra.Command, args []string) error {
	key, value := strings.TrimSpace(args[0]), strings.TrimSpace(args[1])
	spec, ok := findConfigKeySpec(key)
	if !ok {
		return unknownConfigKeyError(key)
	}
	parsed, err := parseConfigValue(spec, value)
	if err != nil {
		return err
	}

	path := selectedConfigPath()
	root, err := loadConfigYAMLNode(path)
	if err != nil {
		return err
	}
	if err := setYAMLPath(root, strings.Split(key, "."), parsed); err != nil {
		return err
	}
	data, err := yaml.Marshal(root)
	if err != nil {
		return fmt.Errorf("marshaling config: %w", err)
	}
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return fmt.Errorf("creating config directory: %w", err)
	}
	if err := os.WriteFile(path, data, 0o644); err != nil {
		return fmt.Errorf("writing config %s: %w", path, err)
	}
	_, err = fmt.Fprintf(cmd.OutOrStdout(), "Set %s = %s in %s\n", key, value, path)
	return err
}

func runConfigGet(cmd *cobra.Command, args []string) error {
	key := strings.TrimSpace(args[0])
	if _, ok := findConfigKeySpec(key); !ok {
		return unknownConfigKeyError(key)
	}
	root, err := loadConfigYAMLNode(selectedConfigPath())
	if err != nil {
		return err
	}
	node := getYAMLPath(root, strings.Split(key, "."))
	if node == nil || node.Value == "" {
		_, err = fmt.Fprintf(cmd.OutOrStdout(), "%s: not set\n", key)
		return err
	}
	_, err = fmt.Fprintf(cmd.OutOrStdout(), "%s: %s\n", key, node.Value)
	return err
}

func runConfigToggle(cmd *cobra.Command, args []string) error {
	key := strings.TrimSpace(args[0])
	spec, ok := findConfigKeySpec(key)
	if !ok {
		return unknownConfigKeyError(key)
	}
	if spec.Type != configTypeBool {
		return fmt.Errorf("config key %q is not a boolean", key)
	}

	root, err := loadConfigYAMLNode(selectedConfigPath())
	if err != nil {
		return err
	}
	current := false
	if node := getYAMLPath(root, strings.Split(key, ".")); node != nil && node.Value != "" {
		parsed, err := strconv.ParseBool(node.Value)
		if err != nil {
			return fmt.Errorf("parsing current value for %s: expected bool", key)
		}
		current = parsed
	}
	return runConfigSet(cmd, []string{key, strconv.FormatBool(!current)})
}

func runConfigKeys(cmd *cobra.Command, args []string) error {
	specs := append([]configKeySpec(nil), configKeySpecs...)
	sort.Slice(specs, func(i, j int) bool {
		return specs[i].Key < specs[j].Key
	})
	for _, spec := range specs {
		if _, err := fmt.Fprintf(cmd.OutOrStdout(), "%-24s %-8s %s\n", spec.Key, spec.Type, spec.Description); err != nil {
			return err
		}
	}
	return nil
}

func findConfigKeySpec(key string) (configKeySpec, bool) {
	for _, spec := range configKeySpecs {
		if spec.Key == key {
			return spec, true
		}
	}
	return configKeySpec{}, false
}

func unknownConfigKeyError(key string) error {
	return fmt.Errorf("unknown config key %q; run `jevkit config keys`", key)
}

func parseConfigValue(spec configKeySpec, value string) (*yaml.Node, error) {
	if spec.Validate != nil {
		if err := spec.Validate(value); err != nil {
			return nil, fmt.Errorf("invalid value for %s: %w", spec.Key, err)
		}
	}

	node := &yaml.Node{Kind: yaml.ScalarNode}
	switch spec.Type {
	case configTypeString:
		node.Tag = "!!str"
		node.Value = value
	case configTypeBool:
		parsed, err := strconv.ParseBool(value)
		if err != nil {
			return nil, fmt.Errorf("invalid value for %s: expected bool", spec.Key)
		}
		node.Tag = "!!bool"
		node.Value = strconv.FormatBool(parsed)
	case configTypeInt:
		parsed, err := strconv.Atoi(value)
		if err != nil {
			return nil, fmt.Errorf("invalid value for %s: expected int", spec.Key)
		}
		node.Tag = "!!int"
		node.Value = strconv.Itoa(parsed)
	case configTypeDuration:
		if _, err := time.ParseDuration(value); err != nil {
			return nil, fmt.Errorf("invalid value for %s: expected duration: %w", spec.Key, err)
		}
		node.Tag = "!!str"
		node.Value = value
	default:
		return nil, fmt.Errorf("unsupported config value type %q", spec.Type)
	}
	return node, nil
}

func oneOf(allowed ...string) func(string) error {
	return func(value string) error {
		for _, candidate := range allowed {
			if value == candidate {
				return nil
			}
		}
		return fmt.Errorf("expected one of: %s", strings.Join(allowed, ", "))
	}
}

func loadConfigYAMLNode(path string) (*yaml.Node, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		if !os.IsNotExist(err) {
			return nil, fmt.Errorf("reading config: %w", err)
		}
		return emptyYAMLDocument(), nil
	}
	if strings.TrimSpace(string(data)) == "" {
		return emptyYAMLDocument(), nil
	}
	var root yaml.Node
	if err := yaml.Unmarshal(data, &root); err != nil {
		return nil, fmt.Errorf("parsing config %s: %w", path, err)
	}
	if root.Kind == 0 {
		return emptyYAMLDocument(), nil
	}
	return &root, nil
}

func emptyYAMLDocument() *yaml.Node {
	return &yaml.Node{
		Kind:    yaml.DocumentNode,
		Content: []*yaml.Node{{Kind: yaml.MappingNode}},
	}
}

func setYAMLPath(root *yaml.Node, parts []string, value *yaml.Node) error {
	if len(parts) == 0 {
		return fmt.Errorf("config key cannot be empty")
	}
	mapping, err := rootMapping(root)
	if err != nil {
		return err
	}
	for len(parts) > 1 {
		key := parts[0]
		child := mappingValue(mapping, key)
		if child == nil {
			child = &yaml.Node{Kind: yaml.MappingNode}
			mapping.Content = append(mapping.Content, &yaml.Node{Kind: yaml.ScalarNode, Tag: "!!str", Value: key}, child)
		}
		if child.Kind != yaml.MappingNode {
			child.Kind = yaml.MappingNode
			child.Tag = ""
			child.Value = ""
			child.Content = nil
		}
		mapping = child
		parts = parts[1:]
	}
	key := parts[0]
	if existing := mappingValue(mapping, key); existing != nil {
		*existing = *value
		return nil
	}
	mapping.Content = append(mapping.Content, &yaml.Node{Kind: yaml.ScalarNode, Tag: "!!str", Value: key}, value)
	return nil
}

func getYAMLPath(root *yaml.Node, parts []string) *yaml.Node {
	if len(parts) == 0 {
		return nil
	}
	mapping, err := rootMapping(root)
	if err != nil {
		return nil
	}
	for i, key := range parts {
		node := mappingValue(mapping, key)
		if node == nil {
			return nil
		}
		if i == len(parts)-1 {
			return node
		}
		if node.Kind != yaml.MappingNode {
			return nil
		}
		mapping = node
	}
	return nil
}

func rootMapping(root *yaml.Node) (*yaml.Node, error) {
	if root.Kind == yaml.DocumentNode {
		if len(root.Content) == 0 {
			root.Content = append(root.Content, &yaml.Node{Kind: yaml.MappingNode})
		}
		if root.Content[0].Kind != yaml.MappingNode {
			return nil, fmt.Errorf("config root must be a mapping")
		}
		return root.Content[0], nil
	}
	if root.Kind == yaml.MappingNode {
		return root, nil
	}
	return nil, fmt.Errorf("config root must be a mapping")
}

func mappingValue(mapping *yaml.Node, key string) *yaml.Node {
	for i := 0; i+1 < len(mapping.Content); i += 2 {
		if mapping.Content[i].Value == key {
			return mapping.Content[i+1]
		}
	}
	return nil
}
