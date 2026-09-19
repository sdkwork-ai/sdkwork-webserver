package types


type ServerOperationResult struct {
	OperationId string `json:"operationId"`
	ExitCode int `json:"exitCode"`
	TimedOut bool `json:"timedOut"`
	Stdout string `json:"stdout"`
	Stderr string `json:"stderr"`
	StdoutTruncated bool `json:"stdoutTruncated"`
	StderrTruncated bool `json:"stderrTruncated"`
	Pid int `json:"pid"`
	PidFile string `json:"pidFile"`
	LogFile string `json:"logFile"`
	Stopped bool `json:"stopped"`
	Message string `json:"message"`
}
