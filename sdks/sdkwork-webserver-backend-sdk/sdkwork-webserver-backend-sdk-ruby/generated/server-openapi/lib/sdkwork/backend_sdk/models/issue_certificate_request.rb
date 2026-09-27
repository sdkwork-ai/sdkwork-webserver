module Sdkwork
  module BackendSdk
    module Models
      class IssueCertificateRequest
              attr_accessor :domain_ids, :cert_type, :key_algorithm, :auto_renew, :cert_name, :certificate_scope, :validation_method, :renew_before_days, :ca_profile, :provider_account_id

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @domain_ids = attributes['domainIds'].is_a?(Array) ? attributes['domainIds'].map { |item| item } : []
                @cert_type = attributes['certType']
                @key_algorithm = attributes['keyAlgorithm']
                @auto_renew = attributes['autoRenew']
                @cert_name = attributes['certName']
                @certificate_scope = attributes['certificateScope']
                @validation_method = attributes['validationMethod']
                @renew_before_days = attributes['renewBeforeDays']
                @ca_profile = attributes['caProfile']
                @provider_account_id = attributes['providerAccountId']
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
                  'certName' => @cert_name,
                  'certificateScope' => @certificate_scope,
                  'validationMethod' => @validation_method,
                  'renewBeforeDays' => @renew_before_days,
                  'caProfile' => @ca_profile,
                  'providerAccountId' => @provider_account_id,
                }
              end
            end
    end
  end
end
