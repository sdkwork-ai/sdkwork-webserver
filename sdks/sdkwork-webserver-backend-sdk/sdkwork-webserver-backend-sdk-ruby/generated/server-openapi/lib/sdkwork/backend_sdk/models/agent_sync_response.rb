module Sdkwork
  module BackendSdk
    module Models
      class AgentSyncResponse
              attr_accessor :server_id, :sync_version, :unchanged, :nginx_configs, :certificates

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @server_id = attributes['serverId']
                @sync_version = attributes['syncVersion']
                @unchanged = attributes['unchanged']
                @nginx_configs = attributes['nginxConfigs'].is_a?(Array) ? attributes['nginxConfigs'].map { |item| item.is_a?(Hash) ? AgentNginxConfigBundle.from_hash(item) : item } : []
                @certificates = attributes['certificates'].is_a?(Array) ? attributes['certificates'].map { |item| item.is_a?(Hash) ? AgentCertificateBundle.from_hash(item) : item } : []
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'serverId' => @server_id,
                  'syncVersion' => @sync_version,
                  'unchanged' => @unchanged,
                  'nginxConfigs' => @nginx_configs.is_a?(Array) ? @nginx_configs.map { |item| item.respond_to?(:to_hash) ? item.to_hash : item } : [],
                  'certificates' => @certificates.is_a?(Array) ? @certificates.map { |item| item.respond_to?(:to_hash) ? item.to_hash : item } : [],
                }
              end
            end
    end
  end
end
