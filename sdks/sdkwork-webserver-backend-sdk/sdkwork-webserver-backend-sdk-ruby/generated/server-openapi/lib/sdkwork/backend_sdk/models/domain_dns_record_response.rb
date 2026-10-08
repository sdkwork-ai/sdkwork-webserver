module Sdkwork
  module BackendSdk
    module Models
      class DomainDnsRecordResponse
              attr_accessor :id, :record_name, :record_type, :record_value, :ttl_seconds, :priority, :record_line, :domain_id, :dns_provider, :cloud_account_id, :provider_record_ref, :synced_at

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @id = attributes['id']
                @record_name = attributes['recordName']
                @record_type = attributes['recordType']
                @record_value = attributes['recordValue']
                @ttl_seconds = attributes['ttlSeconds']
                @priority = attributes['priority']
                @record_line = attributes['recordLine']
                @domain_id = attributes['domainId']
                @dns_provider = attributes['dnsProvider']
                @cloud_account_id = attributes['cloudAccountId']
                @provider_record_ref = attributes['providerRecordRef']
                @synced_at = attributes['syncedAt']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'id' => @id,
                  'recordName' => @record_name,
                  'recordType' => @record_type,
                  'recordValue' => @record_value,
                  'ttlSeconds' => @ttl_seconds,
                  'priority' => @priority,
                  'recordLine' => @record_line,
                  'domainId' => @domain_id,
                  'dnsProvider' => @dns_provider,
                  'cloudAccountId' => @cloud_account_id,
                  'providerRecordRef' => @provider_record_ref,
                  'syncedAt' => @synced_at,
                }
              end
            end
    end
  end
end
