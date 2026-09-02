module Sdkwork
  module AppSdk
    module Models
      class ListenerCertificateSummaryResponse
              attr_accessor :cert_name, :identifiers, :issuer, :fingerprint, :not_after, :status

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @cert_name = attributes['certName']
                @identifiers = attributes['identifiers'].is_a?(Array) ? attributes['identifiers'].map { |item| item.is_a?(Hash) ? CertificateIdentifierResponse.from_hash(item) : item } : []
                @issuer = attributes['issuer']
                @fingerprint = attributes['fingerprint']
                @not_after = attributes['notAfter']
                @status = attributes['status']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'certName' => @cert_name,
                  'identifiers' => @identifiers.is_a?(Array) ? @identifiers.map { |item| item.respond_to?(:to_hash) ? item.to_hash : item } : [],
                  'issuer' => @issuer,
                  'fingerprint' => @fingerprint,
                  'notAfter' => @not_after,
                  'status' => @status,
                }
              end
            end
    end
  end
end
