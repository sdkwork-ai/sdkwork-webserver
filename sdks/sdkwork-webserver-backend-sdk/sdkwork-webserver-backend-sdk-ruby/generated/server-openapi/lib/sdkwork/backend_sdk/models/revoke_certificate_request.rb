module Sdkwork
  module BackendSdk
    module Models
      class RevokeCertificateRequest
              attr_accessor :reason

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @reason = attributes['reason']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'reason' => @reason,
                }
              end
            end
    end
  end
end
