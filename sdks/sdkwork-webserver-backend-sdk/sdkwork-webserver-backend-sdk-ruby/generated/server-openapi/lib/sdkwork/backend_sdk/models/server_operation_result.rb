module Sdkwork
  module BackendSdk
    module Models
      class ServerOperationResult
              attr_accessor :operation_id, :exit_code, :timed_out, :stdout, :stderr, :stdout_truncated, :stderr_truncated, :pid, :pid_file, :log_file, :stopped, :message

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @operation_id = attributes['operationId']
                @exit_code = attributes['exitCode']
                @timed_out = attributes['timedOut']
                @stdout = attributes['stdout']
                @stderr = attributes['stderr']
                @stdout_truncated = attributes['stdoutTruncated']
                @stderr_truncated = attributes['stderrTruncated']
                @pid = attributes['pid']
                @pid_file = attributes['pidFile']
                @log_file = attributes['logFile']
                @stopped = attributes['stopped']
                @message = attributes['message']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'operationId' => @operation_id,
                  'exitCode' => @exit_code,
                  'timedOut' => @timed_out,
                  'stdout' => @stdout,
                  'stderr' => @stderr,
                  'stdoutTruncated' => @stdout_truncated,
                  'stderrTruncated' => @stderr_truncated,
                  'pid' => @pid,
                  'pidFile' => @pid_file,
                  'logFile' => @log_file,
                  'stopped' => @stopped,
                  'message' => @message,
                }
              end
            end
    end
  end
end
