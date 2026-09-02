module Sdkwork
  module BackendSdk
    module Models
      class CertificateResponse
              attr_accessor :id, :cert_name, :identifiers, :cert_type, :issuer, :fingerprint, :key_algorithm, :not_before, :not_after, :auto_renew, :renewal_status, :status, :created_at

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @id = attributes['id']
                @cert_name = attributes['certName']
                @identifiers = attributes['identifiers'].is_a?(Array) ? attributes['identifiers'].map { |item| item.is_a?(Hash) ? CertificateIdentifierResponse.from_hash(item) : item } : []
                @cert_type = attributes['certType']
                @issuer = attributes['issuer']
                @fingerprint = attributes['fingerprint']
                @key_algorithm = attributes['keyAlgorithm']
                @not_before = attributes['notBefore']
                @not_after = attributes['notAfter']
                @auto_renew = attributes['autoRenew']
                @renewal_status = attributes['renewalStatus']
                @status = attributes['status']
                @created_at = attributes['createdAt']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'id' => @id,
                  'certName' => @cert_name,
                  'identifiers' => @identifiers.is_a?(Array) ? @identifiers.map { |item| item.respond_to?(:to_hash) ? item.to_hash : item } : [],
                  'certType' => @cert_type,
                  'issuer' => @issuer,
                  'fingerprint' => @fingerprint,
                  'keyAlgorithm' => @key_algorithm,
                  'notBefore' => @not_before,
                  'notAfter' => @not_after,
                  'autoRenew' => @auto_renew,
                  'renewalStatus' => @renewal_status,
                  'status' => @status,
                  'createdAt' => @created_at,
                }
              end
            end
    end
  end
end
