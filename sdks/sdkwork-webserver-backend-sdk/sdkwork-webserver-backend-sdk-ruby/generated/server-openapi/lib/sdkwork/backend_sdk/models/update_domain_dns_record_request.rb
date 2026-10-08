module Sdkwork
  module BackendSdk
    module Models
      class UpdateDomainDnsRecordRequest
              attr_accessor :record_type, :host, :record_value, :ttl_seconds, :priority, :record_line

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @record_type = attributes['recordType']
                @host = attributes['host']
                @record_value = attributes['recordValue']
                @ttl_seconds = attributes['ttlSeconds']
                @priority = attributes['priority']
                @record_line = attributes['recordLine']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'recordType' => @record_type,
                  'host' => @host,
                  'recordValue' => @record_value,
                  'ttlSeconds' => @ttl_seconds,
                  'priority' => @priority,
                  'recordLine' => @record_line,
                }
              end
            end
    end
  end
end
