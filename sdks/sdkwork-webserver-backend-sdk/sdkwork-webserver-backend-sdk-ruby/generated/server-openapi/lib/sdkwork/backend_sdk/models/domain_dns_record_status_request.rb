module Sdkwork
  module BackendSdk
    module Models
      class DomainDnsRecordStatusRequest
              attr_accessor :enabled

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @enabled = attributes['enabled']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'enabled' => @enabled,
                }
              end
            end
    end
  end
end
