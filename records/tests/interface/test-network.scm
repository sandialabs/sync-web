(lambda (make-interface-harness)

  (with-let (make-interface-harness :journals 4 :users '(alice))

    (define error-result?
      (lambda (result)
        (and (pair? result) (eq? (car result) 'error))))

    ;; Installation is ordinary scheduled work and must settle before the
    ;; journals begin publishing signed heads.
    (test-report)

    (for-each
      (lambda (journal)
        (test-submit ((*journal* journal 'set!) '(*state* network seed) (journal 'name)) :expect #t)
        (test-submit ((*journal* journal 'step!)) :expect 1))
      (list journal-1 journal-2 journal-3 journal-4))
    (test-report)

    ;; Establish two independent edges concurrently with reversed response
    ;; timing, then connect them through the middle edge.
    (test-submit ((*journal* journal-1 'bridge!) journal-2) :schedule '(3 1) :expect #t)
    (test-submit ((*journal* journal-3 'bridge!) journal-4) :schedule '(1 3) :expect #t)
    (test-report)

    (test-submit ((*journal* journal-1 'config) '((path (private bridge journal-2 initiation)))) :expect 'local)
    (test-submit ((*journal* journal-2 'config) '((path (private bridge journal-1 initiation)))) :expect 'remote)
    (test-submit ((*journal* journal-3 'config) '((path (private bridge journal-4 initiation)))) :expect 'local)
    (test-submit ((*journal* journal-4 'config) '((path (private bridge journal-3 initiation)))) :expect 'remote)
    (test-submit ((*journal* journal-2 'bridge!) journal-3) :schedule '(2 2) :expect #t)
    (test-report)

    ;; Commit each received head, then propagate journal-4's nested view back
    ;; through journal-3 and journal-2 into journal-1.
    (for-each
      (lambda (journal)
        (test-submit ((*journal* journal 'step!)) :expect 2))
      (list journal-1 journal-2 journal-3 journal-4))
    (test-report)

    (test-submit ((*journal* journal-2 'bridge!) journal-3) :schedule '(2 1) :expect #t)
    (test-report)
    (test-submit ((*journal* journal-2 'step!)) :expect 3)
    (test-submit ((*journal* journal-1 'bridge!) journal-2) :schedule '(1 2) :tick 1 :expect #t)
    (test-report)
    (test-submit ((*journal* journal-1 'step!)) :expect 3)
    (test-report)

    (test-submit
      ((*anonymous* journal-1 'route)
       '((route-target (journal-2 journal-3 journal-4))))
      :schedule '(1 2 1 0 2 1)
      :expect (lambda (result)
                (and (equal? (cadr (assoc 'route-source result))
                             '(journal-1 journal-2 journal-3))
                     (integer? (cadr (assoc 'terminal-index result)))
                     (list? (cadr (assoc 'object result))))))

    ;; Independent scheduled synchronization may overlap while both ends stage
    ;; ordinary writes. Each step still commits one coherent local head.
    (test-submit ((*journal* journal-1 'set!) '(*state* network round) "origin") :expect #t)
    (test-submit ((*journal* journal-4 'set!) '(*state* network round) "terminal") :expect #t)
    (test-submit ((*journal* journal-3 'bridge!) journal-4) :schedule '(3 2) :expect #t)
    (test-submit ((*journal* journal-1 'step!)) :schedule '(1 3) :tick 1 :expect 4)
    (test-submit ((*journal* journal-4 'step!)) :tick 1 :expect 3)
    (test-report)

    (test-submit ((*journal* journal-3 'step!)) :expect 3)
    (test-submit ((*journal* journal-2 'bridge!) journal-3) :schedule '(2 1) :tick 1 :expect #t)
    (test-report)
    (test-submit ((*journal* journal-2 'step!)) :expect 4)
    (test-submit ((*journal* journal-1 'bridge!) journal-2) :schedule '(1 2) :tick 1 :expect #t)
    (test-report)
    (test-submit ((*journal* journal-1 'step!)) :expect 5)
    (test-report)

    (test-submit
      ((*anonymous* journal-1 'route)
       '((route-target (journal-2 journal-3 journal-4))))
      :schedule '(2 1 0 2 1 0)
      :expect (lambda (result)
                (and (equal? (cadr (assoc 'route-source result))
                             '(journal-1 journal-2 journal-3))
                     (list? (cadr (assoc 'object result))))))

    ;; Re-establishing the same active relationship remains idempotent even
    ;; when two normal administrative requests overlap in flight.
    (test-submit ((*journal* journal-1 'bridge!) journal-2) :schedule '(3 1) :expect #t)
    (test-submit ((*journal* journal-1 'bridge!) journal-2) :schedule '(1 2) :tick 1 :expect #t)
    (test-submit
      ((*journal* journal-1 'config)
       '((path (private bridge journal-2 public-key))))
      :tick 1 :expect byte-vector?)
    (test-report)

    ;; Give Alice a direct application path whose signing key can be observed
    ;; across two ordinary interface-key rotations.
    (test-submit ((*journal* journal-2 'set!) '(*state* network rotation) "available") :expect #t)
    (test-submit
      ((*journal* journal-2 'authorize!)
       '((user (*state* network))
         (rule ((principal (journal-1 *state* alice))
                (key-index (-20 -1)) (path (rotation))
                (get #t) (set! #f) (resolve #t)))))
      :expect #t)
    (test-submit ((*journal* journal-2 'step!)) :expect 5)
    (test-submit ((*journal* journal-1 'bridge!) journal-2) :schedule '(2 1) :tick 1 :expect #t)
    (test-report)
    (test-submit ((*journal* journal-1 'step!)) :expect 6)
    (test-submit
      ((alice journal-1 journal-2 'get) '(*state* network rotation))
      :schedule '(1 2 0 1) :expect "available")
    (test-report)

    (define (rotate! secret marker expected-origin-size expected-terminal-size)
      (test-submit ((*journal* journal-1 '*secret*) `((secret ,secret))) :expect #t)
      (test-report)
      (journal-1 'credentials secret)
      ;; Before the new key commits, federation still uses the retained key
      ;; matching the terminal's committed reverse view.
      (test-submit
        ((alice journal-1 journal-2 'get) '(*state* network rotation))
        :schedule '(2 1 0 1) :expect "available")
      (test-submit ((*journal* journal-1 'set!) `(*state* alice ,marker) marker) :expect #t)
      (test-submit ((*journal* journal-1 'step!)) :schedule '(1 2) :tick 1 :expect expected-origin-size)
      (test-report)
      ;; Committing retires the old private key before the peer has committed
      ;; the new public key, so the stale route is temporarily unusable.
      (test-submit
        ((alice journal-1 journal-2 'get) '(*state* network rotation))
        :schedule '(1 2 0 1) :expect error-result?)
      (test-submit ((*journal* journal-1 'bridge!) journal-2) :schedule '(2 1) :tick 1 :expect #t)
      (test-report)
      (test-submit ((*journal* journal-2 'step!)) :expect expected-terminal-size)
      (test-submit ((*journal* journal-1 'bridge!) journal-2) :schedule '(1 2) :tick 1 :expect #t)
      (test-report)
      (test-submit ((*journal* journal-1 'step!)) :expect (+ expected-origin-size 1))
      (test-submit
        ((alice journal-1 journal-2 'get) '(*state* network rotation))
        :schedule '(2 1 0 1) :tick 1 :expect "available")
      (test-report))

    (rotate! "network-credential-2" 'rotation-1 7 6)
    (rotate! "network-credential-3" 'rotation-2 9 7)

    (test-report))

  ;; Simultaneous reciprocal establishment is a recoverable administrative
  ;; race, not a permanent role inversion. Both failed halves roll back, after
  ;; which one ordinary retry establishes exactly one ongoing initiator.
  (with-let (make-interface-harness :journals 2 :journal-start 10 :users '(alice))

    (define (error-result? result)
      (and (pair? result) (eq? (car result) 'error)))

    (test-report)
    (test-submit ((*journal* journal-10 'set!) '(*state* seed) "ten") :expect #t)
    (test-submit ((*journal* journal-11 'set!) '(*state* seed) "eleven") :expect #t)
    (test-submit ((*journal* journal-10 'step!)) :expect 1)
    (test-submit ((*journal* journal-11 'step!)) :expect 1)
    (test-report)

    (test-submit ((*journal* journal-10 'bridge!) journal-11) :schedule '(0 2) :expect error-result?)
    (test-submit ((*journal* journal-11 'bridge!) journal-10) :schedule '(0 2) :expect error-result?)
    (test-report)
    (test-submit ((*journal* journal-10 'config) '((path (private bridge journal-11)))) :expect '())
    (test-submit ((*journal* journal-11 'config) '((path (private bridge journal-10)))) :expect '())
    (test-submit ((*journal* journal-10 'bridge!) journal-11) :expect #t)
    (test-report)
    (test-submit ((*journal* journal-10 'config) '((path (private bridge journal-11 initiation)))) :expect 'local)
    (test-submit ((*journal* journal-11 'config) '((path (private bridge journal-10 initiation)))) :expect 'remote)

    (test-report))

  ;; A valid second-phase continuation is single-state-bound: the first apply
  ;; succeeds, while replay after state advancement is rejected without change.
  (with-let (make-interface-harness :journals 2 :journal-start 12 :users '(alice))
    (define (error-result? result)
      (and (pair? result) (eq? (car result) 'error)))
    (define (collect action)
      (test-submit action)
      (test-await))
    (test-submit ((*journal* journal-12 'set!) '(*state* seed) "twelve")
                 :expect #t)
    (test-submit ((*journal* journal-13 'set!) '(*state* seed) "thirteen")
                 :expect #t)
    (test-submit ((*journal* journal-12 'step!)) :expect 1)
    (test-submit ((*journal* journal-13 'step!)) :expect 1)
    (test-report)
    (let* ((result
            (collect
             (raw journal-12
                  `(*call* "pass-12"
                           (lambda (root)
                             (let ((ledger
                                    (sync-eval
                                     ((root 'get) '(root object ledger))))
                                   (federation
                                    (sync-eval
                                     ((root 'get) '(root object federation)))))
                               ((federation 'bridge!)
                                ledger 'journal-13
                                '((interface "http://journal-13.test/interface")
                                  (remote-name journal-12)))))))))
           (operation-data (cadr (assoc 'continuation result)))
           (request
            `((function bridge!)
              (arguments
               ((name journal-13)
                (interface "http://journal-13.test/interface")
                (remote-name journal-12)))
              (authentication
               ((credentials ,(journal-12 'credentials))))
              (prepared ,operation-data))))
      (test-submit (raw journal-12 request) :expect #t)
      (test-submit (raw journal-12 request) :expect error-result?)
      (test-submit
       ((*journal* journal-12 'config)
        '((path (private bridge journal-13 initiation))))
       :expect 'local))
    (test-report))

  ;; Stable random journal identities salt root-secret-derived signing keys.
  ;; Existing peers verify only missing K0 -> K1 -> K2 transitions after
  ;; skipping both rotations, while history and the relationship survive.
  (with-let (make-interface-harness :journals 2 :journal-start 20 :users '(alice bob))

    (define (error-result? result)
      (and (pair? result) (eq? (car result) 'error)))

    (define (collect action)
      (test-report)
      (test-submit action)
      (test-await))

    (define (derived-key journal root-secret common-secret)
      (collect
       (raw journal
            `(*call* ,root-secret
                     (lambda (root)
                       (let* ((ledger
                               (sync-eval ((root 'get) '(root object ledger))))
                              (identity
                               (cadr
                                (assoc 'identity
                                       (cadr (assoc 'public
                                                    ((ledger 'config)))))))
                              (identity-id (cadr (assoc 'id identity))))
                         (car
                          (crypto-generate
                           (expression->byte-vector
                            (list 'sync-web/journal-signing-key/v1
                                  identity-id
                                  (sync-hash
                                   (expression->byte-vector ,common-secret))))))))))))

    (define (identity-digest journal root-secret nonce)
      (collect
       (raw journal
            `(*call* ,root-secret
                     (lambda (root)
                       (sync-hash
                        (expression->byte-vector
                         (list 'sync-web/journal-id/v1 ',nonce))))))))

    (define (signed-head journal root-secret)
      (collect
       (raw journal
            `(*call* ,root-secret
                     (lambda (root)
                       (((sync-eval ((root 'get) '(root object ledger)))
                         'signed-head) -1))))))

    (test-report)
    (test-submit ((*journal* journal-20 'set!) '(*state* alice seed) "origin") :expect #t)
    (test-submit ((*journal* journal-21 'set!) '(*state* bob document) "stable") :expect #t)
    (test-submit
      ((*journal* journal-21 'authorize!)
       '((user (*state* bob))
         (rule ((principal (journal-20 *state* alice))
                (key-index (-20 -1)) (path (document))
                (get #t) (set! #f) (resolve #t)))))
      :expect #t)
    (test-submit ((*journal* journal-20 'step!)) :expect 1)
    (test-submit ((*journal* journal-21 'step!)) :expect 1)
    (test-report)

    (let* ((info-20 (collect ((*anonymous* journal-20 'info))))
           (info-21 (collect ((*anonymous* journal-21 'info))))
           (identity-20 (cadr (assoc 'identity info-20)))
           (identity-21 (cadr (assoc 'identity info-21)))
           (identity-id-20 (cadr (assoc 'id identity-20)))
           (computed-id-20
            (identity-digest journal-20 "pass-20"
                             (cadr (assoc 'nonce identity-20))))
           (initial-key (cadr (assoc 'public-key info-20)))
           (initial-head (signed-head journal-20 "pass-20"))
           (same-secret-key-20
            (derived-key journal-20 "pass-20" "shared root password"))
           (same-secret-key-21
            (derived-key journal-21 "pass-21" "shared root password")))
      (test-submit
        ((*anonymous* journal-20 'info))
        :expect (lambda (result)
                  (and (= (length identity-20) 2)
                       (= (length identity-id-20) 32)
                       (= (length (cadr (assoc 'nonce identity-20))) 32)
                       (equal? identity-id-20 computed-id-20))))
      (test-submit
        ((*anonymous* journal-21 'info))
        :expect (lambda (result)
                  (not (equal? (cadr (assoc 'id identity-20))
                               (cadr (assoc 'id identity-21))))))
      (test-submit
        ((*anonymous* journal-20 'info))
        :expect (lambda (result)
                  (not (equal? same-secret-key-20 same-secret-key-21))))

      (test-submit ((*journal* journal-20 'bridge!) journal-21) :expect #t)
      (test-report)
      (test-submit ((*journal* journal-21 'step!)) :expect 2)
      (test-report)
      (test-submit ((*journal* journal-20 'bridge!) journal-21) :expect #t)
      (test-report)
      (test-submit
        (raw journal-20 '(*step* "pass-20" (ledger-step #t)))
        :expect 2)
      (test-submit
        ((alice journal-20 journal-21 'get) '(*state* bob document))
        :expect "stable")
      (test-report)

      (test-submit
        (raw journal-20 '(*set-secret* "pass-20" "root-20-v2"))
        :expect #t)
      (test-report)
      (test-submit (raw journal-20 '(*step* "pass-20")) :expect error-result?)
      (journal-20 'root-secret "root-20-v2")
      (test-submit
        (raw journal-20 '(*step* "root-20-v2" (ledger-step #t)))
        :expect 3)
      (test-report)
      (test-submit
        (raw journal-20 '(*set-secret* "root-20-v2" "root-20-v3"))
        :expect #t)
      (test-report)
      (journal-20 'root-secret "root-20-v3")
      (test-submit
        (raw journal-20 '(*step* "root-20-v3" (ledger-step #t)))
        :expect 4)
      (test-report)
      (test-submit
        (raw journal-20
             '(*call* "root-20-v3"
                       (lambda (root)
                         (let* ((ledger
                                 (sync-eval
                                  ((root 'get) '(root object ledger))))
                                (config ((ledger 'config)))
                                (identity
                                 (cadr
                                  (assoc 'identity
                                         (cadr (assoc 'public config)))))
                                (index
                                 (car
                                  (reverse
                                   (cadr
                                    (assoc 'rotation-indexes
                                           (cadr
                                            (assoc 'journal
                                                   (cadr
                                                    (assoc 'private
                                                           config)))))))))
                                (rotation
                                 ((ledger 'resolve) index
                                  `(*crypto* journal rotation) #f))
                                (forged
                                 (map (lambda (entry)
                                        (if (eq? (car entry) 'signature)
                                            '(signature #u(1 2 3)) entry))
                                      rotation)))
                           ((ledger '~rotation-verify)
                            identity forged
                            (cadr (assoc 'previous-key rotation)))))))
        :expect error-result?)

      (let* ((rotated-info (collect ((*anonymous* journal-20 'info))))
             (rotated-identity (cadr (assoc 'identity rotated-info)))
             (rotated-key (cadr (assoc 'public-key rotated-info))))
        (test-submit
          ((*anonymous* journal-20 'info))
          :expect (lambda (result) (equal? identity-20 rotated-identity)))
        (test-submit
          ((*anonymous* journal-20 'info))
          :expect (lambda (result) (not (equal? initial-key rotated-key))))
        ;; A first-contact proof exposes the current signed head but leaves old
        ;; rotation certificates unmaterialized; a new peer accepts that current
        ;; key directly as its initial checkpoint.
        (test-submit
          (raw journal-20
               '(*call* "root-20-v3"
                         (lambda (root)
                           (let* ((standard
                                   (sync-eval
                                    ((root 'get) '(root object standard))))
                                  (ledger
                                   (sync-eval
                                    ((root 'get) '(root object ledger))))
                                  (federation
                                   (sync-eval
                                    ((root 'get) '(root object federation))))
                                  (head
                                   ((standard 'deserialize)
                                    ((ledger 'signed-head) -1)))
                                  (config ((ledger 'config)))
                                  (identity-id
                                   (cadr
                                    (assoc 'id
                                           (cadr
                                            (assoc 'identity
                                                   (cadr
                                                    (assoc 'public
                                                           config))))))))
                             (list
                              ((standard 'deep-get)
                               head '(2 (*crypto* journal rotation)))
                              ((federation '~signature-verify)
                               head identity-id #f -1))))))
          :expect (lambda (result)
                    (and (equal? (car result) '(unknown))
                         (equal? (cadr result) rotated-key))))
        (test-submit
          ((*journal* journal-20 'resolve)
           '(0 *state* alice seed) :pinned? #f :proof? #f)
          :expect "origin")

        ;; Journal-21 last accepted K0. One ordinary exchange verifies both
        ;; missed transitions and advances it directly to K2.
        (test-submit
          ((*journal* journal-21 'config)
           '((path (private bridge journal-20 public-key))))
          :expect initial-key)
        (test-submit
          ((*journal* journal-20 'resolve)
           '(2 *crypto* journal rotation) :pinned? #f :proof? #f)
          :expect (lambda (rotation)
                    (equal? (cadr (assoc 'previous-key rotation)) initial-key)))
        (test-submit ((*journal* journal-20 'bridge!) journal-21) :expect #t)
        (test-report)
        (test-submit
          ((*journal* journal-21 'config)
           '((path (private bridge journal-20 public-key))))
          :expect rotated-key)
        (test-submit
          ((*journal* journal-21 'config)
           '((path (private bridge-identity journal-20))))
          :expect identity-id-20)
        (test-submit
          (raw journal-21
               `(*call* "pass-21"
                         (lambda (root)
                           (let* ((standard
                                   (sync-eval
                                    ((root 'get) '(root object standard))))
                                  (ledger
                                   (sync-eval
                                    ((root 'get) '(root object ledger))))
                                  (federation
                                   (sync-eval
                                    ((root 'get) '(root object federation))))
                                  (old-head
                                   ((standard 'deserialize) ',initial-head)))
                             ((federation '~signature-verify)
                              old-head ',identity-id-20 ',rotated-key
                              (cadr
                               (assoc 'last-index
                                      (cadr
                                       (assoc 'journal-20
                                              (cadr
                                               (assoc 'bridge
                                                      (cadr
                                                       (assoc 'private
                                                              ((ledger 'config)))))))))))))))
          :expect error-result?)
        (test-submit ((*journal* journal-21 'step!)) :expect 3)
        (test-submit ((*journal* journal-20 'bridge!) journal-21) :expect #t)
        (test-report)
        (test-submit
          (raw journal-20 '(*step* "root-20-v3" (ledger-step #t)))
          :expect 5)
        (test-submit
          ((alice journal-20 journal-21 'get) '(*state* bob document))
          :expect "stable")
        (test-report)

        ;; Rotation is symmetric: when the acceptor rotates, the initiator
        ;; validates the returned lineage through bridge-synchronize!.
        (test-submit
          (raw journal-21 '(*set-secret* "pass-21" "root-21-v2"))
          :expect #t)
        (test-report)
        (journal-21 'root-secret "root-21-v2")
        (let* ((rotated-21 (collect ((*anonymous* journal-21 'info))))
               (rotated-key-21 (cadr (assoc 'public-key rotated-21))))
          (test-submit ((*journal* journal-20 'bridge!) journal-21) :expect #t)
          (test-report)
          (test-submit
            ((*journal* journal-20 'config)
             '((path (private bridge journal-21 public-key))))
            :expect rotated-key-21)
          (test-submit
            (raw journal-20 '(*step* "root-20-v3" (ledger-step #t)))
            :expect 6)
          (test-submit
            ((alice journal-20 journal-21 'get) '(*state* bob document))
            :expect "stable")

          ;; A refused reinstall leaves retired crypto continuity intact.
          (test-submit
            ((*journal* journal-20 'delete-bridge!) 'journal-21)
            :expect #t)
          (test-submit
            (update-interface journal-20 '())
            :expect error-result?)
          (test-submit
            ((*journal* journal-20 'config)
             '((path (private bridge-retired journal-21 public-key))))
            :expect rotated-key-21)
          (test-submit ((*journal* journal-20 'bridge!) journal-21)
                       :expect #t)
          (test-report)
          (test-submit
            ((*journal* journal-20 'config)
             '((path (private bridge journal-21 public-key))))
            :expect rotated-key-21)
          (test-submit
            ((alice journal-20 journal-21 'get) '(*state* bob document))
            :expect "stable"))))

    (test-report)))
