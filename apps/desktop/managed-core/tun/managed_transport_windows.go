package tun

import (
	"context"
	"errors"
	"fmt"
	"net"
	"time"
	"unsafe"

	winio "github.com/tailscale/go-winio"
	"golang.org/x/sys/windows"
)

var managedServerPID = windows.NewLazySystemDLL("kernel32.dll").NewProc("GetNamedPipeServerProcessId")

type managedTokenIdentity struct {
	user    *windows.Tokenuser
	session uint32
	logon   uint64
}

func managedIdentity(process windows.Handle) (managedTokenIdentity, error) {
	var token windows.Token
	if err := windows.OpenProcessToken(process, windows.TOKEN_QUERY, &token); err != nil {
		return managedTokenIdentity{}, err
	}
	defer token.Close()
	user, err := token.GetTokenUser()
	if err != nil {
		return managedTokenIdentity{}, err
	}
	var returned, session, elevation uint32
	if err := windows.GetTokenInformation(token, windows.TokenSessionId, (*byte)(unsafe.Pointer(&session)), 4, &returned); err != nil {
		return managedTokenIdentity{}, err
	}
	if returned != 4 {
		return managedTokenIdentity{}, errors.New("invalid managed peer session token")
	}
	if err := windows.GetTokenInformation(token, windows.TokenElevation, (*byte)(unsafe.Pointer(&elevation)), 4, &returned); err != nil {
		return managedTokenIdentity{}, err
	}
	if returned != 4 || elevation != 1 {
		return managedTokenIdentity{}, errors.New("managed core channel requires an elevated helper and core")
	}
	// TOKEN_STATISTICS starts with TokenId and AuthenticationId (two LUIDs).
	// Aligned, bounded storage also accommodates the complete native structure.
	var statistics [16]uint64
	if err := windows.GetTokenInformation(token, windows.TokenStatistics, (*byte)(unsafe.Pointer(&statistics[0])), uint32(unsafe.Sizeof(statistics)), &returned); err != nil {
		return managedTokenIdentity{}, err
	}
	if returned < 16 || returned > uint32(unsafe.Sizeof(statistics)) {
		return managedTokenIdentity{}, errors.New("invalid managed peer authentication token")
	}
	return managedTokenIdentity{user, session, statistics[1]}, nil
}

func authenticateManagedHelper(conn net.Conn, config managedConfig) (windows.Handle, error) {
	file, ok := conn.(interface{ Fd() uintptr })
	if !ok {
		return 0, errors.New("managed pipe does not expose an authenticatable native handle")
	}
	var pid uint32
	result, _, callErr := managedServerPID.Call(file.Fd(), uintptr(unsafe.Pointer(&pid)))
	if result == 0 {
		return 0, fmt.Errorf("query managed pipe server: %w", callErr)
	}
	if pid != config.HelperPID {
		return 0, errors.New("managed pipe server is not the launching helper")
	}
	parent, err := windows.OpenProcess(windows.PROCESS_QUERY_LIMITED_INFORMATION|windows.SYNCHRONIZE, false, pid)
	if err != nil {
		return 0, err
	}
	complete := false
	defer func() {
		if !complete {
			windows.CloseHandle(parent)
		}
	}()
	var created, exited, kernel, user windows.Filetime
	if err := windows.GetProcessTimes(parent, &created, &exited, &kernel, &user); err != nil {
		return 0, err
	}
	if uint64(created.HighDateTime)<<32|uint64(created.LowDateTime) != config.HelperCreated {
		return 0, errors.New("managed helper process creation identity changed")
	}
	peer, err := managedIdentity(parent)
	if err != nil {
		return 0, err
	}
	current, err := managedIdentity(windows.CurrentProcess())
	if err != nil {
		return 0, err
	}
	if !peer.user.User.Sid.Equals(current.user.User.Sid) || peer.session != current.session || peer.logon != current.logon {
		return 0, errors.New("managed helper belongs to another user/logon/session")
	}
	state, err := windows.WaitForSingleObject(parent, 0)
	if err != nil {
		return 0, err
	}
	if state != uint32(windows.WAIT_TIMEOUT) {
		return 0, errors.New("managed helper exited during authentication")
	}
	complete = true
	return parent, nil
}

func connectManagedSession(config managedConfig) (*managedSession, error) {
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	// Exact data/query rights exclude FILE_CREATE_PIPE_INSTANCE. Identification
	// prevents the server from using the client's token for impersonated work.
	conn, err := winio.DialPipeAccessImpLevel(ctx, config.Pipe, 0x12019b, winio.PipeImpLevelIdentification)
	if err != nil {
		return nil, err
	}
	parent, err := authenticateManagedHelper(conn, config)
	if err != nil {
		conn.Close()
		return nil, err
	}
	return &managedSession{config: config, conn: conn, parent: parent}, nil
}
