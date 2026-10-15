module Sdkwork
  module BackendSdk
    module Models
      class TunnelMachineResponse
              attr_accessor :device_id, :device_name, :platform, :online, :enabled, :session_id, :first_seen_at, :last_seen_at, :connected_at, :disconnected_at, :remote_addr, :bytes_in, :bytes_out, :sessions_total, :route_count, :healthy_route_count, :unhealthy_route_ids, :routes

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @device_id = attributes['deviceId']
                @device_name = attributes['deviceName']
                @platform = attributes['platform']
                @online = attributes['online']
                @enabled = attributes['enabled']
                @session_id = attributes['sessionId']
                @first_seen_at = attributes['firstSeenAt']
                @last_seen_at = attributes['lastSeenAt']
                @connected_at = attributes['connectedAt']
                @disconnected_at = attributes['disconnectedAt']
                @remote_addr = attributes['remoteAddr']
                @bytes_in = attributes['bytesIn']
                @bytes_out = attributes['bytesOut']
                @sessions_total = attributes['sessionsTotal']
                @route_count = attributes['routeCount']
                @healthy_route_count = attributes['healthyRouteCount']
                @unhealthy_route_ids = attributes['unhealthyRouteIds'].is_a?(Array) ? attributes['unhealthyRouteIds'].map { |item| item } : []
                @routes = attributes['routes'].is_a?(Array) ? attributes['routes'].map { |item| item.is_a?(Hash) ? TunnelMachineRouteResponse.from_hash(item) : item } : []
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'deviceId' => @device_id,
                  'deviceName' => @device_name,
                  'platform' => @platform,
                  'online' => @online,
                  'enabled' => @enabled,
                  'sessionId' => @session_id,
                  'firstSeenAt' => @first_seen_at,
                  'lastSeenAt' => @last_seen_at,
                  'connectedAt' => @connected_at,
                  'disconnectedAt' => @disconnected_at,
                  'remoteAddr' => @remote_addr,
                  'bytesIn' => @bytes_in,
                  'bytesOut' => @bytes_out,
                  'sessionsTotal' => @sessions_total,
                  'routeCount' => @route_count,
                  'healthyRouteCount' => @healthy_route_count,
                  'unhealthyRouteIds' => @unhealthy_route_ids.is_a?(Array) ? @unhealthy_route_ids.map { |item| item } : [],
                  'routes' => @routes.is_a?(Array) ? @routes.map { |item| item.respond_to?(:to_hash) ? item.to_hash : item } : [],
                }
              end
            end
    end
  end
end
