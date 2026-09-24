module Sdkwork
  module BackendSdk
    module Models
      class UpdateRootDomainRequest
              # Partial edit; an omitted member leaves the stored value unchanged. The apex hostname is not editable.
              attr_accessor :display_name, :dns_provider, :provider_zone_ref, :status

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @display_name = attributes['displayName']
                @dns_provider = attributes['dnsProvider']
                @provider_zone_ref = attributes['providerZoneRef']
                @status = attributes['status']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'displayName' => @display_name,
                  'dnsProvider' => @dns_provider,
                  'providerZoneRef' => @provider_zone_ref,
                  'status' => @status,
                }
              end
            end
    end
  end
end
