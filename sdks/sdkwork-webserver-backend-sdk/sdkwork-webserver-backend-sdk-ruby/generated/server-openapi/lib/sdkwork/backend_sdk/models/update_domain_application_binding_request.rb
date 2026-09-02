module Sdkwork
  module BackendSdk
    module Models
      class UpdateDomainApplicationBindingRequest
              attr_accessor :application_id, :is_primary

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @application_id = attributes['applicationId']
                @is_primary = attributes['isPrimary']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'applicationId' => @application_id,
                  'isPrimary' => @is_primary,
                }
              end
            end
    end
  end
end
