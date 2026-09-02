module Sdkwork
  module BackendSdk
    module Models
      class CreateServerResponse
              attr_accessor :id, :name, :host, :tenant_scope_hash, :ssh_port, :status, :last_heartbeat_at, :created_at, :agent_token

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @id = attributes['id']
                @name = attributes['name']
                @host = attributes['host']
                @tenant_scope_hash = attributes['tenantScopeHash']
                @ssh_port = attributes['sshPort']
                @status = attributes['status']
                @last_heartbeat_at = attributes['lastHeartbeatAt']
                @created_at = attributes['createdAt']
                @agent_token = attributes['agentToken']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'id' => @id,
                  'name' => @name,
                  'host' => @host,
                  'tenantScopeHash' => @tenant_scope_hash,
                  'sshPort' => @ssh_port,
                  'status' => @status,
                  'lastHeartbeatAt' => @last_heartbeat_at,
                  'createdAt' => @created_at,
                  'agentToken' => @agent_token,
                }
              end
            end
    end
  end
end
