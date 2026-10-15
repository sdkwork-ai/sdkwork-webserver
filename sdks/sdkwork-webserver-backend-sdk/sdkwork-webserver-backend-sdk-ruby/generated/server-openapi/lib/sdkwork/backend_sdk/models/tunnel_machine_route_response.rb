module Sdkwork
  module BackendSdk
    module Models
      class TunnelMachineRouteResponse
              attr_accessor :route_id, :name, :protocol, :domain, :port, :secret_name, :locations, :load_balancer_group, :bandwidth_limit_bytes_per_second, :proxy_protocol, :allow_public, :auth, :allowed_ips, :healthy

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @route_id = attributes['routeId']
                @name = attributes['name']
                @protocol = attributes['protocol']
                @domain = attributes['domain']
                @port = attributes['port']
                @secret_name = attributes['secretName']
                @locations = attributes['locations'].is_a?(Array) ? attributes['locations'].map { |item| item } : []
                @load_balancer_group = attributes['loadBalancerGroup']
                @bandwidth_limit_bytes_per_second = attributes['bandwidthLimitBytesPerSecond']
                @proxy_protocol = attributes['proxyProtocol']
                @allow_public = attributes['allowPublic']
                @auth = attributes['auth']
                @allowed_ips = attributes['allowedIps'].is_a?(Array) ? attributes['allowedIps'].map { |item| item } : []
                @healthy = attributes['healthy']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'routeId' => @route_id,
                  'name' => @name,
                  'protocol' => @protocol,
                  'domain' => @domain,
                  'port' => @port,
                  'secretName' => @secret_name,
                  'locations' => @locations.is_a?(Array) ? @locations.map { |item| item } : [],
                  'loadBalancerGroup' => @load_balancer_group,
                  'bandwidthLimitBytesPerSecond' => @bandwidth_limit_bytes_per_second,
                  'proxyProtocol' => @proxy_protocol,
                  'allowPublic' => @allow_public,
                  'auth' => @auth,
                  'allowedIps' => @allowed_ips.is_a?(Array) ? @allowed_ips.map { |item| item } : [],
                  'healthy' => @healthy,
                }
              end
            end
    end
  end
end
