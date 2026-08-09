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

    (test-submit
      ((*journal* journal-3 'authorize!)
       '((user (*state* network))
         (rule ((principal (journal-2 journal-1 *state* alice))
                (key-index (-20 -1)) (path (seed))
                (get #t) (set! #t) (resolve #t)))))
      :expect #t)

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

    ;; A forward route is not yet invocable when its exact terminal object does
    ;; not contain the complete reverse source-key path. It fails without a
    ;; terminal effect until ordinary reciprocal synchronization commits that
    ;; evidence back through the route.
    (test-submit
      ((alice journal-1 journal-2 journal-3 'get) '(*state* network seed))
      :expect error-result?)
    (test-submit
      ((*journal* journal-3 'get) '(*state* network seed))
      :expect 'journal-3)
    (test-report)
    (test-submit ((*journal* journal-3 'step!)) :expect 3)
    (test-submit ((*journal* journal-2 'bridge!) journal-3)
                 :schedule '(2 1) :tick 1 :expect #t)
    (test-report)
    (test-submit ((*journal* journal-2 'step!)) :expect 4)
    (test-submit ((*journal* journal-1 'bridge!) journal-2)
                 :schedule '(1 2) :tick 1 :expect #t)
    (test-report)
    (test-submit ((*journal* journal-1 'step!)) :expect 4)
    (test-report)

    ;; After convergence, current two-hop get, set!, and resolve use the one
    ;; deterministic journal-bound interface key without persisted key lists.
    (test-submit
      ((alice journal-1 journal-2 journal-3 'get) '(*state* network seed))
      :expect 'journal-3)
    (test-submit
      ((alice journal-1 journal-2 journal-3 'set!)
       '(*state* network seed) "two-hop" :expected 'journal-3)
      :expect #t)
    (test-submit
      ((alice journal-1 journal-2 journal-3 'set!)
       '(*state* network seed) "wrong" :expected 'journal-3)
      :expect #f)
    (test-submit
      ((alice journal-1 journal-2 journal-3 'get) '(*state* network seed))
      :expect "two-hop")
    (test-submit
      ((alice journal-1 journal-2 journal-3 'resolve)
       '(-1 *state* network seed) :pinned? #f :proof? #f)
      :expect 'journal-3)
    (test-submit
      ((alice journal-1 'resolve-batch)
       '((-1 journal-2 -1 journal-3 -1 *state* network seed)
         (-1 journal-2 -1 journal-3 -1 *state* network seed)))
      :expect
      '((results
         (((path (-1 journal-2 -1 journal-3 -1 *state* network seed))
           (content journal-3))
          ((path (-1 journal-2 -1 journal-3 -1 *state* network seed))
           (content journal-3))))))
    (test-report)

    ;; Independent scheduled synchronization may overlap while both ends stage
    ;; ordinary writes. Each step still commits one coherent local head.
    (test-submit ((*journal* journal-1 'set!) '(*state* network round) "origin") :expect #t)
    (test-submit ((*journal* journal-4 'set!) '(*state* network round) "terminal") :expect #t)
    (test-submit ((*journal* journal-3 'bridge!) journal-4) :schedule '(3 2) :expect #t)
    (test-submit ((*journal* journal-1 'step!)) :schedule '(1 3) :tick 1 :expect 5)
    (test-submit
      ((alice journal-1 journal-2 journal-3 'get) '(*state* network seed))
      :tick 1 :expect "two-hop")
    (test-submit
      ((alice journal-1 journal-2 journal-3 'set!) '(*state* network seed) "advanced")
      :tick 1 :expect #t)
    (test-submit
      ((alice journal-1 journal-2 journal-3 'resolve)
       '(-1 *state* network seed) :pinned? #f :proof? #f)
      :tick 1 :expect 'journal-3)
    (test-submit ((*journal* journal-4 'step!)) :tick 1 :expect 3)
    (test-report)

    (test-submit ((*journal* journal-3 'step!)) :expect 4)
    (test-submit ((*journal* journal-2 'bridge!) journal-3) :schedule '(2 1) :tick 1 :expect #t)
    (test-report)
    (test-submit ((*journal* journal-2 'step!)) :expect 5)
    (test-submit ((*journal* journal-1 'bridge!) journal-2) :schedule '(1 2) :tick 1 :expect #t)
    (test-report)
    (test-submit ((*journal* journal-1 'step!)) :expect 6)
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
                (get #t) (set! #t) (resolve #t)))))
      :expect #t)
    (test-submit ((*journal* journal-2 'step!)) :expect 6)
    (test-submit ((*journal* journal-1 'bridge!) journal-2) :schedule '(2 1) :tick 1 :expect #t)
    (test-report)
    (test-submit ((*journal* journal-1 'step!)) :expect 7)
    (test-submit
      ((alice journal-1 journal-2 'get) '(*state* network rotation))
      :schedule '(1 2 0 1) :expect "available")
    (test-report)

    (define (rotate! secret marker expected-origin-size expected-terminal-size)
      (test-submit ((*journal* journal-1 '*secret*) `((secret ,secret))) :expect #t)
      (test-report)
      (journal-1 'credentials secret)
      ;; Rotation immediately removes access to the old signing capability.
      ;; Until the replacement public key propagates through a committed peer
      ;; view, the stale route fails closed without an old-key fallback.
      (test-submit
        ((alice journal-1 journal-2 'get) '(*state* network rotation))
        :schedule '(2 1 0 1) :expect error-result?)
      (test-submit
        ((alice journal-1 journal-2 'set!)
         '(*state* network rotation) "uncommitted")
        :schedule '(2 1 0 1) :expect error-result?)
      (test-submit
        ((alice journal-1 journal-2 'resolve)
         '(-1 *state* network rotation) :pinned? #f :proof? #f)
        :schedule '(2 1 0 1) :expect error-result?)
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
      (test-submit
        ((alice journal-1 journal-2 'set!)
         '(*state* network rotation) "available")
        :schedule '(2 1 0 1) :expect #t)
      (test-submit
        ((alice journal-1 journal-2 'resolve)
         '(-1 *state* network rotation) :pinned? #f :proof? #f)
        :schedule '(2 1 0 1) :expect "available")
      (test-report))

    (rotate! "network-credential-2" 'rotation-1 8 7)
    (rotate! "network-credential-3" 'rotation-2 10 8)

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
                             (let* ((ledger
                                     (sync-eval
                                      ((root 'get) '(root object ledger))))
                                    (federation
                                     (sync-eval
                                      ((root 'get) '(root object federation))))
                                    (identity ((ledger 'config) '(public identity)))
                                    (id (cadr (assoc 'id identity)))
                                    (key
                                     (crypto-generate
                                      (expression->byte-vector
                                       (list 'sync-web/federation-continuation-signing-key/v1
                                             id
                                             (sync-hash
                                              (expression->byte-vector
                                               ,(journal-12 'url))))))))
                               ((federation 'bridge!)
                                ledger 'journal-13
                                '((interface "http://journal-13.test/interface")
                                  (remote-name journal-12)) key)))))))
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
        (raw journal-20
             '(*set-secret* "pass-20" "http://journal-20.test/interface"))
        :expect error-result?)
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
        (raw journal-20
             '(*call* "root-20-v3"
                      (lambda (root)
                        ((root 'get) '(interface credential)))))
        :expect "http://journal-20.test/interface")
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

      ;; Sparse rotation export follows only the strictly decreasing committed
      ;; predecessor chain. Missing, self-looping, and increasing links fail
      ;; before serialization rather than scanning or looping over config state.
      (test-submit
       (raw journal-20
            '(*call* "root-20-v3"
              (lambda (root)
                (let ((standard
                       (sync-eval ((root 'get) '(root object standard))))
                      (stored ((root 'get) '(root object ledger))))
                  (define (tag thunk)
                    (catch #t
                      (lambda () (thunk) 'unexpected-success)
                      (lambda args (car args))))
                  (define (malformed previous)
                    (let* ((ledger (sync-eval stored))
                           (latest
                            ((ledger 'config)
                             '(public journal latest-rotation-index)))
                           (rotation
                            ((ledger 'resolve)
                             `(,latest *crypto* journal rotation) #f))
                           (replacement
                            (map (lambda (entry)
                                   (if (eq? (car entry) 'previous-index)
                                       (list 'previous-index
                                             (if (eq? previous 'self)
                                                 latest (+ latest 1)))
                                       entry))
                                 rotation))
                           (perm
                            ((standard 'deep-set!)
                             ((ledger '~field!) 'perm)
                             `(,latest (*crypto* journal rotation))
                             replacement)))
                      ((ledger '~field!) 'perm perm)
                      (tag (lambda () ((ledger 'signed-head) 0)))))
                  (let* ((valid-ledger (sync-eval stored))
                         (missing-ledger (sync-eval stored))
                         (latest
                          ((missing-ledger 'config)
                           '(public journal latest-rotation-index))))
                    ((missing-ledger '~config-set!)
                     '(public journal latest-rotation-index) (+ latest 1))
                    (list
                     (list? ((valid-ledger 'signed-head) 0))
                     (tag (lambda () ((missing-ledger 'signed-head) 0)))
                     (malformed 'self)
                     (malformed 'increase)))))))
       :expect '(#t index-error integrity-error integrity-error))
      (test-submit
       (raw journal-20
            `(*call* "root-20-v3"
              (lambda (root)
                (let ((standard
                       (sync-eval ((root 'get) '(root object standard))))
                      (stored-ledger ((root 'get) '(root object ledger)))
                      (federation
                       (sync-eval ((root 'get) '(root object federation)))))
                  (define (tag thunk)
                    (catch #t
                      (lambda () (thunk) 'unexpected-success)
                      (lambda args (car args))))
                  (define (malformed field value)
                    (let* ((ledger (sync-eval stored-ledger))
                           (head
                            ((standard 'deserialize)
                             ((ledger 'signed-head) 0)))
                           (latest
                            ((ledger 'config)
                             '(public journal latest-rotation-index)))
                           (rotation
                            ((standard 'deep-get) head
                             `(,latest (*crypto* journal rotation))))
                           (replacement
                            (map (lambda (entry)
                                   (if (eq? (car entry) field)
                                       (list field
                                             (if (eq? value 'next)
                                                 (+ latest 1) value))
                                       entry))
                                 rotation))
                           (changed
                            ((standard 'deep-set!) head
                             `(,latest (*crypto* journal rotation))
                             replacement)))
                      (tag
                       (lambda ()
                         ((federation '~signature-verify)
                          changed ,identity-id-20 ,initial-key 0)))))
                  (let* ((ledger (sync-eval stored-ledger))
                         (head
                          ((standard 'deserialize)
                           ((ledger 'signed-head) 0)))
                         (current-key
                          ((ledger 'config) '(public journal public-key))))
                    (list
                     (equal?
                      ((federation '~signature-verify)
                       head ,identity-id-20 ,initial-key 0)
                      current-key)
                     (malformed 'index 'next)
                     (malformed 'previous-index 0)
                     (malformed 'previous-key #u(1 2 3))
                     (malformed 'signature #u(1 2 3))))))))
       :expect '(#t integrity-error integrity-error integrity-error
                    integrity-error))

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
