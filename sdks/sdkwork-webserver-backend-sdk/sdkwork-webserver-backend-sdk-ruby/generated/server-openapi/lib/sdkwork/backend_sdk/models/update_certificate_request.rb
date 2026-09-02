module Sdkwork
  module BackendSdk
    module Models
      class UpdateCertificateRequest
              attr_accessor :auto_renew

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @auto_renew = attributes['autoRenew']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'autoRenew' => @auto_renew,
                }
              end
            end
    end
  end
end
