module Sdkwork
  module BackendSdk
    module Models
      class DomainDnsSyncResponse
              attr_accessor :record_count, :synced_at, :zone_apex, :dns_provider, :cloud_account_id

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @record_count = attributes['recordCount']
                @synced_at = attributes['syncedAt']
                @zone_apex = attributes['zoneApex']
                @dns_provider = attributes['dnsProvider']
                @cloud_account_id = attributes['cloudAccountId']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'recordCount' => @record_count,
                  'syncedAt' => @synced_at,
                  'zoneApex' => @zone_apex,
                  'dnsProvider' => @dns_provider,
                  'cloudAccountId' => @cloud_account_id,
                }
              end
            end
    end
  end
end
