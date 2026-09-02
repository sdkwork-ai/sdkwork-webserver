module Sdkwork
  module BackendSdk
    module Models
      class IssueCertificateRequest
              attr_accessor :domain_ids, :cert_type, :key_algorithm, :auto_renew

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @domain_ids = attributes['domainIds'].is_a?(Array) ? attributes['domainIds'].map { |item| item } : []
                @cert_type = attributes['certType']
                @key_algorithm = attributes['keyAlgorithm']
                @auto_renew = attributes['autoRenew']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'domainIds' => @domain_ids.is_a?(Array) ? @domain_ids.map { |item| item } : [],
                  'certType' => @cert_type,
                  'keyAlgorithm' => @key_algorithm,
                  'autoRenew' => @auto_renew,
                }
              end
            end
    end
  end
end
