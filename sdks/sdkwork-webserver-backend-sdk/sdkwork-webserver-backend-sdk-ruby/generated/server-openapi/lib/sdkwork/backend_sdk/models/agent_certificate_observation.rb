module Sdkwork
  module BackendSdk
    module Models
      class AgentCertificateObservation
              attr_accessor :certificate_id, :fingerprint, :sync_version, :state, :observed_at, :failure_code

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @certificate_id = attributes['certificateId']
                @fingerprint = attributes['fingerprint']
                @sync_version = attributes['syncVersion']
                @state = attributes['state']
                @observed_at = attributes['observedAt']
                @failure_code = attributes['failureCode']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'certificateId' => @certificate_id,
                  'fingerprint' => @fingerprint,
                  'syncVersion' => @sync_version,
                  'state' => @state,
                  'observedAt' => @observed_at,
                  'failureCode' => @failure_code,
                }
              end
            end
    end
  end
end
