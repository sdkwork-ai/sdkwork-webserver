module Sdkwork
  module BackendSdk
    module Models
      class CreateTunnelMachineRouteRequest
              attr_accessor :name, :protocol, :domain, :port, :allow_public, :allowed_ips, :auth, :visitor_tokens, :locations, :bandwidth_limit_bytes_per_second, :proxy_protocol, :load_balancer_group

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @name = attributes['name']
                @protocol = attributes['protocol']
                @domain = attributes['domain']
                @port = attributes['port']
                @allow_public = attributes['allowPublic']
                @allowed_ips = attributes['allowedIps'].is_a?(Array) ? attributes['allowedIps'].map { |item| item } : []
                @auth = attributes['auth']
                @visitor_tokens = attributes['visitorTokens'].is_a?(Array) ? attributes['visitorTokens'].map { |item| item } : []
                @locations = attributes['locations'].is_a?(Array) ? attributes['locations'].map { |item| item } : []
                @bandwidth_limit_bytes_per_second = attributes['bandwidthLimitBytesPerSecond']
                @proxy_protocol = attributes['proxyProtocol']
                @load_balancer_group = attributes['loadBalancerGroup']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'name' => @name,
                  'protocol' => @protocol,
                  'domain' => @domain,
                  'port' => @port,
                  'allowPublic' => @allow_public,
                  'allowedIps' => @allowed_ips.is_a?(Array) ? @allowed_ips.map { |item| item } : [],
                  'auth' => @auth,
                  'visitorTokens' => @visitor_tokens.is_a?(Array) ? @visitor_tokens.map { |item| item } : [],
                  'locations' => @locations.is_a?(Array) ? @locations.map { |item| item } : [],
                  'bandwidthLimitBytesPerSecond' => @bandwidth_limit_bytes_per_second,
                  'proxyProtocol' => @proxy_protocol,
                  'loadBalancerGroup' => @load_balancer_group,
                }
              end
            end
    end
  end
end
