package tun

import (
	"bytes"
	"encoding/binary"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net"
	"os"
	"strings"
	"sync"
	"time"

	"github.com/sagernet/sing-tun/internal/winipcfg"
	"github.com/sagernet/sing-tun/internal/wintun"
	"golang.org/x/sys/windows"
)

const managedProtocol = 1
const managedFrameLimit = 16 * 1024

type managedConfig struct {
	Version       uint32 `json:"version"`
	Pipe          string `json:"pipe"`
	Nonce         string `json:"nonce"`
	Interface     string `json:"interface"`
	AdapterGUID   string `json:"adapter_guid"`
	HelperPID     uint32 `json:"helper_pid"`
	HelperCreated uint64 `json:"helper_created"`
}

type managedEvent struct {
	Kind  string `json:"kind"`
	GUID  string `json:"guid,omitempty"`
	LUID  uint64 `json:"luid,omitempty"`
	Index uint32 `json:"index,omitempty"`
}

type managedFrame struct {
	Version  uint32       `json:"version"`
	Sequence uint64       `json:"sequence"`
	Nonce    string       `json:"nonce"`
	Event    managedEvent `json:"event"`
}

type managedAck struct {
	Version  uint32 `json:"version"`
	Sequence uint64 `json:"sequence"`
	Nonce    string `json:"nonce"`
	Accepted bool   `json:"accepted"`
}

type managedSession struct {
	mu       sync.Mutex
	config   managedConfig
	conn     net.Conn
	parent   windows.Handle
	sequence uint64
}

var managedState struct {
	sync.Mutex
	session *managedSession
}

func decodeManagedJSON(data []byte, value any) error {
	decoder := json.NewDecoder(bytes.NewReader(data))
	decoder.DisallowUnknownFields()
	if err := decoder.Decode(value); err != nil {
		return err
	}
	if err := decoder.Decode(new(any)); !errors.Is(err, io.EOF) {
		return errors.New("managed channel contains trailing JSON")
	}
	return nil
}

func managedCreateAdapter(options Options) (*wintun.Adapter, error) {
	managedState.Lock()
	defer managedState.Unlock()
	if managedState.session != nil {
		return nil, errors.New("managed core permits exactly one TUN adapter per process")
	}
	encoded := os.Getenv("LANPATH_MANAGED_SESSION")
	if len(encoded) == 0 || len(encoded) > 4096 {
		return nil, errors.New("TUN requires an authenticated Lanpath managed session")
	}
	var config managedConfig
	if err := decodeManagedJSON([]byte(encoded), &config); err != nil {
		return nil, fmt.Errorf("invalid managed session: %w", err)
	}
	nonce, err := windows.GUIDFromString("{" + config.Nonce + "}")
	if err != nil || nonce == (windows.GUID{}) || config.Nonce != strings.ToLower(strings.Trim(nonce.String(), "{}")) || config.Version != managedProtocol || config.HelperPID == 0 || config.HelperCreated == 0 {
		return nil, errors.New("invalid managed session identity")
	}
	if config.Interface != "LP-GPUI-"+strings.ReplaceAll(config.Nonce, "-", "") || options.Name != config.Interface || config.Pipe != `\\.\pipe\LanpathGPUI-Core-`+config.Nonce {
		return nil, errors.New("managed TUN name/channel mismatch")
	}
	guid, err := windows.GUIDFromString("{" + config.AdapterGUID + "}")
	if err != nil || guid == (windows.GUID{}) {
		return nil, errors.New("invalid managed adapter GUID")
	}
	session, err := connectManagedSession(config)
	if err != nil {
		return nil, err
	}
	complete := false
	defer func() {
		if !complete {
			session.conn.Close()
			windows.CloseHandle(session.parent)
		}
	}()
	if err := session.exchange(managedEvent{Kind: "hello"}); err != nil {
		return nil, err
	}
	// Creation is exclusive in managed mode. Never convert an existing adapter
	// into an ownership receipt, regardless of its name or derived GUID.
	adapter, err := wintun.CreateAdapter(options.Name, TunnelType, &guid)
	if err != nil {
		return nil, fmt.Errorf("create managed adapter (adoption forbidden): %w", err)
	}
	luid := winipcfg.LUID(adapter.LUID())
	actualGUID, err := luid.GUID()
	if err != nil {
		adapter.Close()
		return nil, err
	}
	row, err := luid.Interface()
	if err != nil || *actualGUID != guid || luid == 0 || row.InterfaceIndex == 0 {
		adapter.Close()
		return nil, errors.New("created adapter identity could not be established")
	}
	// This acknowledgement is the durable boundary BEFORE address/DNS/route/MTU
	// configuration. The helper must persist and verify the actual GUID/LUID.
	if err := session.exchange(managedEvent{Kind: "adapter_created", GUID: strings.ToLower(strings.Trim(actualGUID.String(), "{}")), LUID: uint64(luid), Index: row.InterfaceIndex}); err != nil {
		adapter.Close()
		return nil, err
	}
	managedState.session = session
	complete = true
	return adapter, nil
}

func managedFirewallPermission() error {
	managedState.Lock()
	session := managedState.session
	managedState.Unlock()
	if session == nil {
		return errors.New("system stack firewall permission requires a managed TUN session")
	}
	return session.exchange(managedEvent{Kind: "firewall"})
}

// ManagedCoreReady is called only after the complete Box.Start has succeeded.
// Ordinary non-TUN operation has no managed session and no privileged effects.
func ManagedCoreReady() error {
	managedState.Lock()
	session := managedState.session
	managedState.Unlock()
	if session == nil {
		return nil
	}
	return session.exchange(managedEvent{Kind: "ready"})
}

func (s *managedSession) exchange(event managedEvent) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.sequence == ^uint64(0) {
		return errors.New("managed channel sequence exhausted")
	}
	s.sequence++
	if err := s.conn.SetDeadline(time.Now().Add(10 * time.Second)); err != nil {
		return err
	}
	payload, err := json.Marshal(managedFrame{managedProtocol, s.sequence, s.config.Nonce, event})
	if err != nil || len(payload) > managedFrameLimit {
		return errors.New("managed channel frame exceeds bound")
	}
	var size [4]byte
	binary.LittleEndian.PutUint32(size[:], uint32(len(payload)))
	for _, part := range [][]byte{size[:], payload} {
		for len(part) != 0 {
			written, err := s.conn.Write(part)
			if err != nil {
				return err
			}
			if written == 0 {
				return io.ErrShortWrite
			}
			part = part[written:]
		}
	}
	if _, err := io.ReadFull(s.conn, size[:]); err != nil {
		return err
	}
	length := binary.LittleEndian.Uint32(size[:])
	if length == 0 || length > managedFrameLimit {
		return errors.New("invalid managed acknowledgement size")
	}
	response := make([]byte, length)
	if _, err := io.ReadFull(s.conn, response); err != nil {
		return err
	}
	var ack managedAck
	if err := decodeManagedJSON(response, &ack); err != nil {
		return err
	}
	if ack.Version != managedProtocol || ack.Sequence != s.sequence || ack.Nonce != s.config.Nonce || !ack.Accepted {
		return errors.New("managed ownership acknowledgement rejected")
	}
	return nil
}
