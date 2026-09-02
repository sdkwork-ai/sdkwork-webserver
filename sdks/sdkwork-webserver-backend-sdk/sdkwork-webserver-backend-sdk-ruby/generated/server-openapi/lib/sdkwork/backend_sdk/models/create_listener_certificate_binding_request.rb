module Sdkwork
  module BackendSdk
    module Models
      class CreateListenerCertificateBindingRequest
              attr_accessor :certificate_id, :certificate_version_id, :priority, :is_default

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @certificate_id = attributes['certificateId']
                @certificate_version_id = attributes['certificateVersionId']
                @priority = attributes['priority']
                @is_default = attributes['isDefault']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'certificateId' => @certificate_id,
                  'certificateVersionId' => @certificate_version_id,
                  'priority' => @priority,
                  'isDefault' => @is_default,
                }
              end
            end
    end
  end
end
