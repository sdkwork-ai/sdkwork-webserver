module Sdkwork
  module BackendSdk
    module Models
      class AgentHeartbeatRequest
              attr_accessor :agent_version, :nginx_enabled, :active_configs, :last_sync_version, :certificate_observations

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @agent_version = attributes['agentVersion']
                @nginx_enabled = attributes['nginxEnabled']
                @active_configs = attributes['activeConfigs']
                @last_sync_version = attributes['lastSyncVersion']
                @certificate_observations = attributes['certificateObservations'].is_a?(Array) ? attributes['certificateObservations'].map { |item| item.is_a?(Hash) ? AgentCertificateObservation.from_hash(item) : item } : []
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'agentVersion' => @agent_version,
                  'nginxEnabled' => @nginx_enabled,
                  'activeConfigs' => @active_configs,
                  'lastSyncVersion' => @last_sync_version,
                  'certificateObservations' => @certificate_observations.is_a?(Array) ? @certificate_observations.map { |item| item.respond_to?(:to_hash) ? item.to_hash : item } : [],
                }
              end
            end
    end
  end
end
