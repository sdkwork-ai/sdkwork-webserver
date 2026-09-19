module Sdkwork
  module BackendSdk
    module Models
      class ClusterOverviewResponse
              attr_accessor :total_hosts, :online_hosts, :total_instances, :online_instances, :unhealthy_instances, :pending_peer_messages, :generated_at

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @total_hosts = attributes['totalHosts']
                @online_hosts = attributes['onlineHosts']
                @total_instances = attributes['totalInstances']
                @online_instances = attributes['onlineInstances']
                @unhealthy_instances = attributes['unhealthyInstances']
                @pending_peer_messages = attributes['pendingPeerMessages']
                @generated_at = attributes['generatedAt']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'totalHosts' => @total_hosts,
                  'onlineHosts' => @online_hosts,
                  'totalInstances' => @total_instances,
                  'onlineInstances' => @online_instances,
                  'unhealthyInstances' => @unhealthy_instances,
                  'pendingPeerMessages' => @pending_peer_messages,
                  'generatedAt' => @generated_at,
                }
              end
            end
    end
  end
end
