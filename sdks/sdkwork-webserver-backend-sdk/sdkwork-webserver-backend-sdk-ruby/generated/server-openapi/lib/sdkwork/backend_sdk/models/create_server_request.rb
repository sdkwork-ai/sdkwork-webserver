module Sdkwork
  module BackendSdk
    module Models
      class CreateServerRequest
              attr_accessor :name, :host, :tenant_scope_hash, :ssh_port

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @name = attributes['name']
                @host = attributes['host']
                @tenant_scope_hash = attributes['tenantScopeHash']
                @ssh_port = attributes['sshPort']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'name' => @name,
                  'host' => @host,
                  'tenantScopeHash' => @tenant_scope_hash,
                  'sshPort' => @ssh_port,
                }
              end
            end
    end
  end
end
