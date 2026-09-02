module Sdkwork
  module BackendSdk
    module Models
      class NginxStatusResponse
              attr_accessor :running, :version, :pid, :active_connections, :config_path, :uptime

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @running = attributes['running']
                @version = attributes['version']
                @pid = attributes['pid']
                @active_connections = attributes['activeConnections']
                @config_path = attributes['configPath']
                @uptime = attributes['uptime']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'running' => @running,
                  'version' => @version,
                  'pid' => @pid,
                  'activeConnections' => @active_connections,
                  'configPath' => @config_path,
                  'uptime' => @uptime,
                }
              end
            end
    end
  end
end
