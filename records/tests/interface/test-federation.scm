(lambda (make-interface-harness)

  (with-let (make-interface-harness
             :journals 7 :journal-start 8 :users '(alice alice-user bob) :tick 1)

  (define (exact journal expression)
    (raw journal expression))

  (define* (collect submitted-action (expect '*no-expect*))
    ;; Exact protocol values are data dependencies between otherwise declarative
    ;; phases. Settle prior work before collecting the one value needed next.
    (test-report)
    (if (eq? expect '*no-expect*)
        (test-submit submitted-action)
        (test-submit submitted-action :expect expect))
    (test-await))

  (define (info journal)
    (collect ((*anonymous* journal 'info))))

  (define error-result?
    (lambda (result)
      (and (pair? result) (eq? (car result) 'error))))


  (define (federation-secret-free? journal root-secret)
    (exact journal
           `(*call* ,root-secret
                     (lambda (root)
                       (let* ((federation
                               (sync-eval
                                ((root 'get) '(root object federation))))
                              (config ((federation 'config))))
                         (and (not (assoc 'keys config))
                              (not (assoc 'secret config))
                              (byte-vector? (cadr (assoc 'public-key config)))
                              (equal? ((root 'get) '(interface private-key))
                                      '(nothing))
                              (equal? ((root 'get) '(interface keys))
                                      '(nothing))))))))

  (define (interface-private-present? journal root-secret secret)
    (exact journal
           `(*call* ,root-secret
                     (lambda (root)
                       (let* ((ledger
                               (sync-eval
                                ((root 'get) '(root object ledger))))
                              (salt ((ledger 'config) '(public key-derivation-salt)))
                              (private-key
                               (cdr
                                (crypto-generate
                                 (expression->byte-vector
                                  (list 'sync-web/interface-signing-key/v1 salt
                                        (sync-hash
                                         (expression->byte-vector ,secret))))))))
                         (let walk ((value (root)))
                           (cond ((byte-vector? value)
                                  (equal? value private-key))
                                 ((sync-pair? value)
                                  (or (walk (sync-car value))
                                      (walk (sync-cdr value))))
                                 (else #f))))))))

  (define (federation-public journal root-secret)
    (exact journal
           `(*call* ,root-secret
                     (lambda (root)
                       (((sync-eval
                          ((root 'get) '(root object federation)))
                         'config)
                        '(interface-public-key))))))

  (define (federation-config-rejection journal root-secret table name)
    (exact journal
           `(*call* ,root-secret
                     (lambda (root)
                       (let* ((federation
                               (sync-eval
                                ((root 'get) '(root object federation))))
                              (before (federation))
                              (before-config ((federation 'config)))
                              (result
                               (catch #t
                                 (lambda ()
                                   ((federation 'update-config!)
                                    (list
                                     (list 'path
                                           (list ',table
                                                 (string->symbol ,name)))
                                     '(value ((marker #t))))))
                                 (lambda args 'rejected))))
                         (list (eq? result 'rejected)
                               (equal? (sync-digest before)
                                       (sync-digest (federation)))
                               (equal? before-config
                                       ((federation 'config)))))))))

  (define (federation-config-set journal root-secret table name value)
    (exact journal
           `(*call* ,root-secret
                     (lambda (root)
                       (let ((federation
                              (sync-eval
                               ((root 'get) '(root object federation)))))
                         ((federation 'update-config!)
                          (list
                           (list 'path (list ',table (string->symbol ,name)))
                           (list 'value ',value)))
                         ((root 'set!) '(root object federation) (federation)))))))

  (define (federation-config-table-rejection journal root-secret table name)
    (exact journal
           `(*call* ,root-secret
                     (lambda (root)
                       (let* ((federation
                               (sync-eval
                                ((root 'get) '(root object federation))))
                              (before (federation))
                              (before-config ((federation 'config)))
                              (result
                               (catch #t
                                 (lambda ()
                                   ((federation 'update-config!)
                                    (list
                                     (list 'path (list ',table))
                                     (list 'value
                                           (list
                                            (list (string->symbol ,name)
                                                  '((marker #t))))))))
                                 (lambda args 'rejected))))
                         (list (eq? result 'rejected)
                               (equal? (sync-digest before)
                                       (sync-digest (federation)))
                               (equal? before-config
                                       ((federation 'config)))))))))

  (define (federation-config-table-set journal root-secret table entries)
    (exact journal
           `(*call* ,root-secret
                     (lambda (root)
                       (let ((federation
                              (sync-eval
                               ((root 'get) '(root object federation)))))
                         ((federation 'update-config!)
                          (list
                           (list 'path (list ',table))
                           (list 'value
                                 (map (lambda (entry)
                                        (list (string->symbol (car entry))
                                              (cadr entry)))
                                      ',entries))))
                         ((root 'set!) '(root object federation) (federation)))))))

  (define (federation-config-get journal root-secret table name)
    (exact journal
           `(*call* ,root-secret
                     (lambda (root)
                       (((sync-eval
                          ((root 'get) '(root object federation)))
                         'config)
                        (list ',table (string->symbol ,name)))))))

  (define (federation-config-table-get journal root-secret table)
    (exact journal
           `(*call* ,root-secret
                     (lambda (root)
                       (((sync-eval
                          ((root 'get) '(root object federation)))
                         'config)
                        (list ',table))))))

  (define (federation-digest journal root-secret)
    (exact journal
           `(*call* ,root-secret
                     (lambda (root)
                       (sync-digest
                        ((root 'get) '(root object federation)))))))

  (define (derived-interface-public journal root-secret secret)
    (exact journal
           `(*call* ,root-secret
                     (lambda (root)
                       (let* ((ledger
                               (sync-eval
                                ((root 'get) '(root object ledger))))
                              (salt ((ledger 'config) '(public key-derivation-salt))))
                         (car
                          (crypto-generate
                           (expression->byte-vector
                            (list 'sync-web/interface-signing-key/v1 salt
                                  (sync-hash
                                   (expression->byte-vector ,secret)))))))))))

  (define* (signed-head journal root-secret (known-index -1))
    (collect
      (exact journal
              `(*call* ,root-secret
                       (lambda (root)
                         (((sync-eval ((root 'get) '(root object ledger)))
                           'signed-head) ,known-index))))))

  (define (signed-head-view journal root-secret cursor indexes)
    (exact journal
      `(*call* ,root-secret
        (lambda (root)
          (let* ((standard (sync-eval ((root 'get) '(root object standard))))
                 (ledger (sync-eval ((root 'get) '(root object ledger))))
                 (chain (sync-eval
                         ((standard 'deserialize) ((ledger 'signed-head) ',cursor)))))
            (list ((chain 'size))
                  (map (lambda (index)
                         (equal? ((chain 'get) index) '(unknown)))
                       ',indexes)
                  (if (>= ,(if (integer? cursor)
                                cursor (cadr (assoc 'index cursor)))
                           ((chain 'size)))
                      #t
                      (byte-vector?
                       ((chain 'digest) ,(if (integer? cursor)
                                             cursor
                                             (cadr (assoc 'index cursor))))))
                  (not (equal? ((chain 'get) -1) '(unknown)))))))))

  (define (reciprocal-bridge journal remote)
    ((*journal* journal 'bridge!) remote))

  (define (bulk-advance journal count)
    (raw journal
      `(*call* ,(journal 'root-secret)
        (lambda (root)
          (let* ((ledger (sync-eval ((root 'get) '(root object ledger))))
                 (key-derivation-salt
                  ((ledger 'config) '(public key-derivation-salt)))
                 (keys
                  (crypto-generate
                   (expression->byte-vector
                    (list 'sync-web/journal-signing-key/v1
                          key-derivation-salt
                          (sync-hash
                           (expression->byte-vector
                            ,(journal 'root-secret))))))))
            (let loop ((i 0))
              (if (< i ,count)
                  (begin
                    ((ledger 'put!) '(*state* bootstrap tick)
                     (expression->byte-vector i))
                    ((ledger 'step!)
                     `((unix-time ,i)
                       (public-key ,(car keys))
                       (secret-key ,(cdr keys))))
                    (loop (+ i 1)))))
            ((root 'set!) '(root object ledger) (ledger))
            ((ledger 'size)))))))

  (define (grant journal owner principal path)
    ((*journal* journal 'authorize!)
     `((user ,owner)
       (rule ((principal ,principal)
              (key-index (-10 -1))
              (path ,path)
              (use! ((read-only? #t)))
              (put! #t)
              (retrieve #t))))))

  (define interface-8 (journal-8 'url))
  (define interface-11 (journal-11 'url))
  (define interface-12 (journal-12 'url))

  ;; Exact protocol tests below occasionally consume prior results to build a
  ;; later signed envelope, so settle automatic installation first.
  (test-report)

  ;; Naming slots reject symbols that cannot survive the exact expression codec,
  ;; while their official percent-encoded equivalents remain ordinary symbols.
  (test-submit
   (exact journal-8
          `(*call* ,(journal-8 'root-secret)
                    (lambda (root)
                      (let ((ledger (sync-eval ((root 'get) '(root object ledger)))))
                        ((ledger 'put!)
                         (list '*state* (string->symbol ".")) #u(1))))))
   :expect error-result?)
  (test-submit
   (exact journal-8
          `(*call* ,(journal-8 'root-secret)
                    (lambda (root)
                      (let ((ledger (sync-eval ((root 'get) '(root object ledger)))))
                        ((ledger 'put!) '(*state* %2E) #u(1))))))
   :expect #t)
  (test-submit
   (exact journal-8
          `(*call* ,(journal-8 'root-secret)
                    (lambda (root)
                      (let ((federation
                             (sync-eval ((root 'get) '(root object federation)))
                            (ledger (sync-eval ((root 'get) '(root object ledger)))))
                        ((federation 'bridge!) ledger (string->symbol ".")
                         '((interface "http://invalid.test/interface")
                           (remote-name safe-peer)) #f))))))
   :expect error-result?)
  (test-submit
   (exact journal-8
          `(*call* ,(journal-8 'root-secret)
                    (lambda (root)
                      (((sync-eval ((root 'get) '(root object federation))) 'peers)))))
   :expect '())

  ;; Interface signing keys are restart-stable for one journal and domain-bound
  ;; to its stable random key-derivation salt. The same operator secret on another
  ;; journal produces a different public key.
  (let ((first
         (collect (derived-interface-public journal-8 "pass-8" interface-8)))
        (again
         (collect (derived-interface-public journal-8 "pass-8" interface-8)))
        (other
         (collect (derived-interface-public journal-9 "pass-9" interface-8)))
        (configured
         (collect (federation-public journal-8 "pass-8"))))
    (test-submit
      (exact journal-8
             `(*call* "pass-8" (lambda (root) (equal? ',first ',again))))
      :expect #t)
    (test-submit
      (exact journal-8
             `(*call* "pass-8" (lambda (root) (equal? ',first ',other))))
      :expect #f)
    (test-submit
      (exact journal-8
             `(*call* "pass-8" (lambda (root) (equal? ',first ',configured))))
      :expect #t))
  (test-submit (federation-secret-free? journal-8 "pass-8") :expect #t)
  (test-submit (interface-private-present? journal-8 "pass-8" interface-8)
               :expect #f)

  ;; The signed message contains the complete ordered batch arguments. Changing
  ;; order, values, codec, or expected presence/content changes the message.
  (test-submit
    (exact journal-8
           '(*call* "pass-8"
             (lambda (root)
               (let* ((federation
                       (sync-eval ((root 'get) '(root object federation))))
                      (invocation
                       '((identity alice-user) (route-source (peer))
                         (route-target ()) (object ()) (terminal-index 0)
                         (signature #u(2))))
                      (arguments
                       '((paths ((*state* one) (*state* two)))
                         (values (#f (nothing)))
                         (expected (#f (nothing))) (expression? #t)))
                      (message
                       ((federation '~invocation-message)
                        'put-batch! arguments invocation)))
                 (and
                  (equal? (cadr (assoc 'arguments message)) arguments)
                  (not
                   (equal?
                    message
                    ((federation '~invocation-message)
                     'put-batch!
                     '((paths ((*state* two) (*state* one)))
                       (values (#f (nothing)))
                       (expected (#f (nothing))) (expression? #t))
                     invocation)))
                  (not
                   (equal?
                    message
                    ((federation '~invocation-message)
                     'put-batch!
                     '((paths ((*state* one) (*state* two)))
                       (values ((nothing) #f))
                       (expected (#f (nothing))) (expression? #t))
                     invocation)))
                  (not
                   (equal?
                    message
                    ((federation '~invocation-message)
                     'put-batch!
                     '((paths ((*state* one) (*state* two)))
                       (values (#f (nothing)))
                       (expected (#f (nothing))) (expression? #f))
                     invocation)))
                  (not
                   (equal?
                    message
                    ((federation '~invocation-message)
                     'put-batch!
                     '((paths ((*state* one) (*state* two)))
                       (values (#f (nothing))) (expression? #t))
                     invocation)))
                  (not
                   (equal?
                    message
                    ((federation '~invocation-message)
                     'put-batch!
                     '((paths ((*state* one) (*state* two)))
                       (values (#f (nothing)))
                       (expected ((nothing) #f)) (expression? #t))
                     invocation))))))))
    :expect #t)

  (test-submit ((*anonymous* journal-8 'info)) :expect (lambda (result) (not (assoc 'bridge-policy result))))
  (test-submit ((*journal* journal-8 'config) '((path (private subscriber)))) :expect '())
  (test-submit ((*anonymous* journal-8 'synchronize)) :expect error-result?)
  (test-submit ((*journal* journal-8 'delete-subscriber!) '((name old))) :expect error-result?)

  ;; A bare remote identity is malformed: federation requires an exact committed
  ;; terminal object, not the removed signature-only authentication shape.
  (test-submit
      (exact journal-8
              '((function use!)
                (arguments ((path (*state* alice seed)) (read-only? #t) (expression? #t)))
                (authentication ((identity (journal-9 *state* alice))))))
      :expect error-result?)

  ;; Establish signed initial heads before reciprocal bridge state is exchanged.
  (test-submit ((*journal* journal-8 'put!) :path '(*state* alice seed) :value "a") :expect #t)
  (test-submit ((*journal* journal-9 'put!) :path '(*state* carol seed) :value "c") :expect #t)
  (test-submit ((*journal* journal-10 'put!) :path '(*state* bob shared) :value "initial") :expect #t)
  (test-submit ((*journal* journal-10 'put!) :path '(*state* bob write-only) :value "locked") :expect #t)
  (test-submit
    ((*journal* journal-10 'put!) :path '(*state* bob published)
     :value "public")
    :expect #t)
  (test-submit ((*journal* journal-10 'put!) :path '(*state* bob hidden) :value "hidden") :expect #t)
  (test-submit ((*journal* journal-10 'put!) :path '(*state* bob *directory*) :value #t) :expect #t)
  (test-submit ((*journal* journal-10 'put!) :path '(*state* nested data public key) :value "public nested") :expect #t)
  (test-submit
    ((*journal* journal-10 'put!)
     :path '(*state* nested data private key) :value "private nested")
    :expect #t)
  (test-submit ((*journal* journal-10 'put!) :path '(*state* nested data hidden key) :value "hidden nested") :expect #t)
  (test-submit ((*journal* journal-8 'step!)) :expect 1)
  (test-submit ((*journal* journal-9 'step!)) :expect 1)
  (test-submit ((*journal* journal-10 'step!)) :expect 1)
  (test-submit ((alice journal-8 'use!) '(*state* alice seed)) :expect "a")
  (test-submit ((*journal* journal-11 'put!) :path '(*state* d seed) :value "d") :expect #t)
  (test-submit ((*journal* journal-12 'put!) :path '(*state* e seed) :value "e") :expect #t)
  (test-submit ((*journal* journal-11 'step!)) :expect 1)
  (test-submit ((*journal* journal-12 'step!)) :expect 1)

  ;; Locked-down acceptors reject unmatched creation without pending state,
  ;; then accept the exact admin-preapproved signing-key hash.
  (test-submit ((*journal* journal-11 'update-config!) '((path (public bridge-accept)) (value preapproved))) :expect #t)
  (test-submit (reciprocal-bridge journal-12 journal-11) :expect error-result?)
  (test-submit ((*journal* journal-11 'config) '((path (private bridge journal-12)))) :expect '())
  (define journal-12-signing-key (cadr (assoc 'public-key (info journal-12))))
  (define journal-12-signing-key-hash
    (collect
     (raw journal-11
          `(*call* "pass-11"
            (lambda (root) (sync-hash ',journal-12-signing-key))))))
  ;; Alias-keyed config is durable expression data rather than Tree state, but
  ;; must enforce the same exact symbol roundtrip admission.
  (test-submit
    ((*journal* journal-11 'update-config!)
     `((path (private bridge-preapproval ,(string->symbol ".")))
       (value ,journal-12-signing-key-hash)))
    :expect error-result?)
  (test-submit
    ((*journal* journal-11 'update-config!)
     `((path (private bridge-preapproval journal-12))
       (value ,journal-12-signing-key-hash)))
    :expect #t)
  (let ((malformed-info
         (map (lambda (entry)
                (if (eq? (car entry) 'public-key)
                    '(public-key #u(0 1 2)) entry))
              (info journal-12))))
    (test-submit
      (exact journal-11
             `((function synchronize!)
               (arguments ((name journal-12)
                           (establish-check? #t)
                           (response ,(signed-head journal-12 "pass-12"))
                           (info ,malformed-info)
                           (interface ,interface-12)
                           (remote-name journal-11)))))
      :expect error-result?))
  (test-submit (exact journal-11
                 `((function synchronize!)
                   (arguments ((name journal-12)
                               (establish-check? #t)
                               (response ,(signed-head journal-12 "pass-12"))
                               (info ,(info journal-12))
                               (interface "https://attacker.invalid/interface")
                               (remote-name journal-11))))) :expect error-result?)
  (test-submit ((*journal* journal-11 'config) '((path (private bridge journal-12)))) :expect '())
  (test-submit (reciprocal-bridge journal-12 journal-11) :expect #t)
  (test-submit ((*journal* journal-11 'config) '((path (private bridge journal-12 initiation)))) :expect 'remote)
  (test-submit
    (exact journal-11
           `((function synchronize!)
             (arguments ((name journal-12) (establish-check? #t)
                         (response ,(signed-head journal-12 "pass-12"))
                         (info ,(info journal-12)) (interface ,interface-12)
                         (remote-name journal-11) (known-index 0)))))
    :expect (lambda (result) (and (assoc 'established? result)
                                  (cadr (assoc 'established? result)))))
  (define retired-principal '(journal-12 *state* eve))
  (test-submit (grant journal-11 '(*state* d) retired-principal '(seed)) :expect #t)
  (test-submit ((*journal* journal-11 '*admins-set*)
                `((admins ((eve ,retired-principal)))))
    :expect error-result?)
  (test-submit ((*journal* journal-11 'delete-bridge!) 'journal-12) :expect #t)
  (test-submit ((*journal* journal-11 'authorizations) '((user (*state* d))))
    :expect pair?)
  (test-submit ((*journal* journal-11 '*admins-get*)) :expect '())
  (test-submit
    ((*journal* journal-11 'config) '((path (private bridge-preapproval journal-12))))
    :expect journal-12-signing-key-hash)
  ;; A matching active initiator repairs the deleted reciprocal side even
  ;; Under preapproval policy, the alias remains reusable without a permanent
  ;; Journal identity binding.
  (test-submit (reciprocal-bridge journal-12 journal-11) :expect #t)
  (test-submit
    ((*journal* journal-11 'config)
     '((path (private bridge journal-12 public-key))))
    :expect (cadr (assoc 'public-key (info journal-12))))
  (test-submit ((*journal* journal-11 'delete-bridge!) 'journal-12) :expect #t)
  (test-submit ((*journal* journal-11 'update-config!) '((path (public bridge-accept)) (value auto))) :expect #t)
  ;; Reusing the alias admits a different signing key under auto policy; retained
  ;; alias-shaped grants deliberately apply to the replacement relationship.
  (test-submit
    (exact journal-11
      `((function synchronize!)
        (arguments ((name journal-12)
                    (response ,(signed-head journal-8 "pass-8"))
                    (info ,(info journal-8))
                    (interface ,interface-8)
                    (remote-name journal-11)))))
    :expect (lambda (result) (cadr (assoc 'ok? result))))
  (test-submit ((*journal* journal-11 'delete-bridge!) 'journal-12) :expect #t)
  (test-submit (reciprocal-bridge journal-12 journal-11) :expect #t)
  ;; A fresh alias accepts the same signing key as an ordinary new relationship.
  (test-submit
    (exact journal-11
      `((function synchronize!)
        (arguments ((name other)
                    (response ,(signed-head journal-12 "pass-12"))
                    (info ,(info journal-12))
                    (interface ,interface-12)
                    (remote-name journal-11)))))
    :expect (lambda (result) (cadr (assoc 'ok? result))))

  ;; Deleting the initiator side preserves the peer's last accepted local index and grants.
  ;; The identical relationship can then prove
  ;; continuity again after crossing log-chain layout boundaries.
  (test-submit (bulk-advance journal-12 31) :expect 32)
  ;; Compact consistency evidence exposes the accepted prefix and current
  ;; signed head without materializing ordinary skipped history.  The legacy
  ;; `through` hint retains the same compact availability.
  (test-submit
    (signed-head-view journal-12 "pass-12" 0 '(1 15 30))
    :expect '(32 (#t #t #t) #t #t))
  (test-submit
    (signed-head-view journal-12 "pass-12" '((index 0) (through 31)) '(1 15 30))
    :expect '(32 (#t #t #t) #t #t))
  ;; A future cursor without an explicit `through` retains the legacy
  ;; head-only response rather than attempting an out-of-range prefix digest.
  (test-submit
    (signed-head-view journal-12 "pass-12" 32 '(0 15 30))
    :expect '(32 (#t #t #t) #t #t))
  (test-submit
    (signed-head-view journal-12 "pass-12" '((index 32)) '(0 15 30))
    :expect '(32 (#t #t #t) #t #t))
  ;; A future-cursor head-only proof cannot satisfy the receiver's actual
  ;; stored prefix and must fail without advancing its accepted index.
  (test-submit
    (exact journal-11
           `((function synchronize!)
             (arguments ((name journal-12)
                         (response ,(signed-head journal-12 "pass-12" 32))
                         (interface ,interface-12)
                         (remote-name journal-11)))))
    :expect error-result?)
  (test-submit
    ((*journal* journal-11 'config)
     '((path (private bridge journal-12 last-index))))
    :expect 0)
  ;; Model an in-flight synchronization accepted after the initiator's last
  ;; acknowledged remote-index. Its response is deliberately not applied at
  ;; the initiator before deletion.
  (test-submit
    (exact journal-11
           `((function synchronize!)
             (arguments ((name journal-12)
                         (response ,(signed-head journal-12 "pass-12" 0))
                         (interface ,interface-12)
                         (remote-name journal-11)))))
    :expect (lambda (result) (and (assoc 'ok? result)
                                  (cadr (assoc 'ok? result)))))
  (test-submit
    ((*journal* journal-11 'config)
     '((path (private bridge journal-12 last-index))))
    :expect 31)
  (test-submit ((*journal* journal-12 'delete-bridge!) 'journal-11) :expect #t)
  (test-submit
    ((*journal* journal-12 'config)
     '((path (private bridge-retired journal-11 head-index))))
    :expect 31)
  (test-submit ((*journal* journal-12 'step!)) :expect 33)
  (test-submit (reciprocal-bridge journal-12 journal-11) :expect #t)
  (test-submit ((*journal* journal-12 'config) '((path (private bridge journal-11 initiation)))) :expect 'local)
  (test-submit (reciprocal-bridge journal-12 journal-11) :expect #t)

  ;; A later deletion refreshes the retained continuity hint rather than
  ;; falling back to the older accepted index.
  (test-submit (bulk-advance journal-12 31) :expect 64)
  (test-submit ((*journal* journal-12 'delete-bridge!) 'journal-11) :expect #t)
  (test-submit ((*journal* journal-12 'step!)) :expect 65)
  (test-submit (reciprocal-bridge journal-12 journal-11) :expect #t)
  (test-submit ((*journal* journal-12 'config) '((path (private bridge-retired journal-11)))) :expect '())

  ;; Bridge creation performs the first reciprocal signed-head exchange and
  ;; records the inverse local-name mapping on the acceptor automatically.
  (test-submit (reciprocal-bridge journal-8 journal-9) :expect #t)
  (test-submit (reciprocal-bridge journal-9 journal-10) :expect #t)
  (test-submit ((*journal* journal-8 'config) '((path (private bridge journal-9 initiation)))) :expect 'local)
  (test-submit ((*journal* journal-9 'config) '((path (private bridge journal-8 initiation)))) :expect 'remote)
  (test-submit ((*journal* journal-9 'config) '((path (private bridge journal-10 initiation)))) :expect 'local)
  (test-submit ((*journal* journal-10 'config) '((path (private bridge journal-9 initiation)))) :expect 'remote)

  (define direct-principal '(journal-8 *state* alice-user))
  (define remote-principal '(journal-9 journal-8 *state* alice-user))
  (define bob-remote-principal '(journal-9 journal-8 *state* bob))
  (test-submit (grant journal-9 '(*state* carol) direct-principal '(seed)) :expect #t)
  (test-submit (grant journal-10 '(*state* bob) remote-principal '(shared)) :expect #t)
  (test-submit (grant journal-10 '(*state* bob) bob-remote-principal '(shared)) :expect #t)
  (test-submit (grant journal-10 '(*state* bob) remote-principal '(copied)) :expect #t)
  (test-submit
    ((*journal* journal-10 'authorize!)
     `((user (*state* bob))
       (rule ((principal ,remote-principal) (key-index (-10 -1))
              (path (write-only))  (put! #t) (retrieve #f)))))
    :expect #t)
  (test-submit
    ((*journal* journal-10 'authorize!)
     '((user (*state* bob))
       (rule ((principal (*public*)) (path (published))
              (use! ((read-only? #t))) (put! #f) (retrieve #t)))))
    :expect #t)
  (test-submit (grant journal-10 '(*state* nested) remote-principal '(data private)) :expect #t)
  (test-submit
    ((*journal* journal-10 'authorize!)
     '((user (*state* nested))
       (rule ((principal (*public*)) (path (data public))
              (use! ((read-only? #t))) (put! #f) (retrieve #t)))))
    :expect #t)
  (test-submit ((*anonymous* journal-10 'use!) '(*state* bob)) :expect (lambda (result)
            (and (eq? (car result) 'directory)
                 (= (length (cadr result)) 4)
                 (assoc 'shared (cadr result))
                 (assoc 'write-only (cadr result))
                 (assoc 'hidden (cadr result))
                 (assoc 'published (cadr result))
                 (not (assoc '*directory* (cadr result))))))
  (test-submit ((*anonymous* journal-10 'use!) '(*state* bob hidden)) :expect error-result?)

  ;; Commit a state graph with intentionally receding nested checkpoints.
  ;; The creation exchanges already staged journal-8 and journal-10 heads in journal-9.
  (test-submit ((*journal* journal-9 'step!)) :expect 2)
  (test-submit ((*journal* journal-8 'step!)) :expect 2)
  (test-submit ((*journal* journal-9 'step!)) :expect 2)
  (test-submit ((*journal* journal-10 'step!)) :expect 2)
  (test-submit ((alice-user journal-8 journal-9 journal-10 'use!) '(*state* bob shared)) :expect error-result?)
  ;; Propagate journal-10's committed reverse view back into an auditable journal-8 head.
  (test-submit ((*journal* journal-9 'step!)) :expect 3)
  (test-submit ((*journal* journal-8 'step!)) :expect 3)
  (test-submit
    ((*journal* journal-8 'retrieve)
     '(-1 journal-9 -1 journal-10 *state* bob shared))
    :expect error-result?)
  (test-submit ((*anonymous* journal-8 'pin!) '(-1 journal-9)) :expect error-result?)
  (test-submit ((*anonymous* journal-8 'unpin!) '(-1 journal-9)) :expect error-result?)

  ;; Finite routes may return to a journal already visited. The existing shallow
  ;; history proves the route shape but cannot yet prove the complete reverse key.
  (collect
    ((*anonymous* journal-8 'route)
     '((route-target (journal-9 journal-8))))
    :expect
    (lambda (result)
      (and (equal? (cadr (assoc 'route-source result)) '(journal-8 journal-9))
           (list? (cadr (assoc 'object result))))))
  (test-submit (grant journal-8 '(*state* alice) remote-principal '(seed)) :expect #t)
  (test-submit ((alice-user journal-8 journal-9 journal-8 'use!) '(*state* alice seed))
    :expect error-result?)

  ;; Ordinary reciprocal generations make the exact reverse route available;
  ;; no root identity or self-route signing shortcut participates.
  (test-submit ((*journal* journal-13 'put!) :path '(*state* alice seed) :value "loop") :expect #t)
  (test-submit ((*journal* journal-14 'put!) :path '(*state* bootstrap seed) :value "peer") :expect #t)
  (test-submit ((*journal* journal-13 'step!)) :expect integer?)
  (test-submit ((*journal* journal-14 'step!)) :expect integer?)
  (test-submit (reciprocal-bridge journal-13 journal-14) :expect #t)
  (test-submit ((*journal* journal-14 'step!)) :expect integer?)
  (test-submit ((*journal* journal-13 'step!)) :expect integer?)
  (test-submit (reciprocal-bridge journal-13 journal-14) :expect #t)
  (test-submit ((*journal* journal-14 'step!)) :expect integer?)
  (test-submit ((*journal* journal-13 'step!)) :expect integer?)
  (test-submit (reciprocal-bridge journal-13 journal-14) :expect #t)
  (test-submit ((*journal* journal-14 'step!)) :expect integer?)
  (test-submit ((*journal* journal-13 'step!)) :expect integer?)
  (define loop-principal '(journal-14 journal-13 *state* alice-user))
  (test-submit (grant journal-13 '(*state* alice) loop-principal '(seed)) :expect #t)
  (test-submit ((alice-user journal-13 journal-14 journal-13 'use!) '(*state* alice seed))
    :expect "loop")

  ;; One public routed read returns journal-10's exact committed object containing
  ;; journal-10-relative access back to journal-8's interface key.
  (let ((routed
         (collect
           ((*anonymous* journal-8 'route)
            '((route-target (journal-9 journal-10))))
           :expect
           (lambda (result)
             (and (equal? (cadr (assoc 'route-source result))
                          '(journal-8 journal-9))
                  (integer? (cadr (assoc 'terminal-index result)))
                  (list? (cadr (assoc 'object result))))))))
    (test-submit (exact journal-10
            `(*call* "pass-10"
                     (lambda (root)
                       (let ((ledger (sync-eval ((root 'get) '(root object ledger))))
                             (standard (sync-eval ((root 'get) '(root object standard)))))
                         (sync-node?
                          ((ledger 'read)
                           ,(cadr (assoc 'terminal-index routed))
                           ((standard 'deserialize)
                            ',(cadr (assoc 'object routed))))))))) :expect #t)
    (test-submit
     (exact journal-10
            `(*call* "pass-10"
                     (lambda (root)
                       (let ((ledger (sync-eval ((root 'get) '(root object ledger))))
                             (standard (sync-eval ((root 'get) '(root object standard)))))
                         ((ledger 'read)
                          ,(cadr (assoc 'terminal-index routed))
                          (sync-cut
                           ((standard 'deserialize)
                            ',(cadr (assoc 'object routed)))))))))
     :expect error-result?)
    (test-submit (exact journal-10
                   `((function use!)
                     (arguments ((path (*state* bob shared)) (read-only? #t) (expression? #t)))
                     (invocation
                      ((identity alice-user)
                       (route-source ,(cadr (assoc 'route-source routed)))
                       (route-target ())
                       (object ,(cadr (assoc 'object routed)))
                       (terminal-index ,(cadr (assoc 'terminal-index routed)))
                       (signature #u(1 2 3)))))) :expect error-result?)
    (test-submit (exact journal-10
                   `((function use-batch!)
                     (arguments ((paths ((*state* bob shared)
                                         (*state* bob shared)))
                                 (expression? #t)))
                     (invocation
                      ((identity alice-user)
                       (route-source ,(cadr (assoc 'route-source routed)))
                       (route-target ())
                       (object ,(cadr (assoc 'object routed)))
                       (terminal-index ,(cadr (assoc 'terminal-index routed)))
                       (signature #u(1 2 3)))))) :expect error-result?))

  ;; The origin authenticates its local scalar identity, signs inside the journal,
  ;; and delivers the private function/arguments directly to the terminal endpoint.
  (test-submit ((alice-user journal-8 journal-9 journal-10 'use!) '(*state* bob shared)) :expect "initial")
  ;; A proof-backed Tree-native value verifies across the direct route.
  (test-submit
    ((bob journal-9 journal-10 'retrieve)
     '(-1 *state* bob published) :pinned? #t :proof? #t)
    :expect
    (lambda (result)
      (and (equal? (cadr (assoc 'content result)) "public")
           (eq? (cadr (assoc 'pinned? result)) #f)
           (not (assoc 'proof-index result))
           (list? (cadr (assoc 'proof result))))))
  (test-submit
    ((alice-user journal-8 journal-9 journal-10 'retrieve
                 :history '(-1 -1 -1))
     '(-1 *state* nested) :pinned? #f :proof? #f)
    :expect (lambda (result)
              (and (eq? (car result) 'directory)
                   (assoc 'data (cadr result)))))
  (test-submit
    ((alice-user journal-8 journal-9 journal-10 'retrieve
                 :history '(-1 -1 -1))
     '(-1 *state* nested) :pinned? #f :proof? #t)
    :expect (lambda (result)
              (and (eq? (car (cadr (assoc 'content result))) 'directory)
                   (assoc 'data (cadr (cadr (assoc 'content result))))
                   (not (assoc 'proof-index result))
                   (list? (cadr (assoc 'proof result))))))
  (let* ((projected
          (collect
            ((alice-user journal-8 journal-9 journal-10 'retrieve
                         :history '(-1 -1 -1))
             '(-1 *state* nested data) :pinned? #f :proof? #t)))
         (proof (cadr (assoc 'proof projected))))
    (test-submit
      (exact journal-8
              `(*call* "pass-8"
                       (lambda (root)
                         (let ((ledger (sync-eval ((root 'get) '(root object ledger))))
                               (standard (sync-eval ((root 'get) '(root object standard)))))
                           ((ledger '~retrieve)
                            '(-1 journal-9 -1 journal-10
                                *state* nested data hidden key)
                            #f #f ((standard 'deserialize) ',proof))))))
      :expect '(unknown))
    (test-submit
      ((alice-user journal-8 journal-9 journal-10 'use!)
       '(*state* nested data hidden key))
      :expect error-result?))
  (test-submit ((alice-user journal-8 journal-9 journal-10 'put!) '(*state* bob shared) "federated") :expect #t)

  (test-submit ((*journal* journal-10 'use!) :path '(*state* bob shared) :expression? #t) :expect "federated")
  (test-submit
    ((alice-user journal-8 journal-9 journal-10 'copy!)
     '((source (*state* bob shared)) (path (*state* bob copied))))
    :expect #t)
  (test-submit ((*journal* journal-10 'use!) '(*state* bob copied)) :expect "federated")
  (test-submit
    ((alice-user journal-8 journal-9 journal-10 'copy-batch!)
     '((sources ((*state* bob shared) (*state* bob copied)))
       (paths ((*state* bob copied) (*state* bob copied)))
       (expected ("federated" "federated")) (expression? #t)))
    :expect #t)
  (test-submit ((*journal* journal-10 'use!) '(*state* bob copied)) :expect "federated")
  (test-submit
    ((alice-user journal-8 journal-9 journal-10 'copy-batch!)
     '((sources ((*state* bob shared) (journal-9 *state* carol seed)))
       (paths ((*state* bob copied) (*state* bob copied)))))
    :expect error-result?)
  (test-submit
    ((alice-user journal-8 journal-9 journal-10 'copy!)
     '((source (journal-9 *state* carol seed)) (path (*state* bob copied))))
    :expect error-result?)

  ;; Staged scalar and dedicated batch access plus committed retrieve may use a
  ;; signed application route. Transition history and protocol/admin operations
  ;; remain local to their journal.
  (test-submit
    ((alice-user journal-8 journal-9 journal-10 'use!)
     '(*transition* operation))
    :expect error-result?)
  (test-submit
    ((alice-user journal-8 journal-9 journal-10 'retrieve)
     '(-1 *transition* operation) :pinned? #f :proof? #f)
    :expect error-result?)
  (test-submit ((alice-user journal-8 journal-9 journal-10 'info)) :expect error-result?)
  (test-submit ((alice-user journal-8 journal-9 journal-10 'size)) :expect error-result?)
  ;; Direct and multi-hop batch invocations preserve exact path order. Duplicate
  ;; writes compare against one prewrite snapshot and apply in input order.
  (test-submit
    ((alice-user journal-8 journal-9 'use-batch!)
     '((*state* carol seed) (*state* carol seed)))
    :expect '("c" "c"))
  (test-submit
    ((alice-user journal-8 journal-9 'put-batch!)
     '((paths ((*state* carol seed)))
       (values ("direct-batch")) (expression? #t)))
    :expect #t)
  (test-submit
    ((alice-user journal-8 journal-9 journal-10 'use-batch!)
     '((*state* bob shared) (*state* bob shared)))
    :expect '("federated" "federated"))
  (test-submit
    ((alice-user journal-8 journal-9 journal-10 'put-batch!)
     '((paths ((*state* bob shared) (*state* bob shared)))
       (values (#f "ordered"))
       (expected ("federated" "federated"))
       (expression? #t)))
    :expect #t)
  (test-submit
    ((alice-user journal-8 journal-9 journal-10 'put-batch!)
     '((paths ((*state* bob shared) (*state* bob shared)))
       (values ("collision-first" "collision-second"))
       (expected ("ordered" "wrong"))
       (expression? #t)))
    :expect #f)
  (test-submit ((*journal* journal-10 'use!) '(*state* bob shared))
               :expect "ordered")
  (test-submit
    ((alice-user journal-8 journal-9 journal-10 'put-batch!)
     '((paths ((*state* bob shared)))
       (values (#f)) (expected ("ordered")) (expression? #t)))
    :expect #t)
  (test-submit
    ((alice-user journal-8 journal-9 journal-10 'put-batch!)
     '((paths ((*state* bob shared)))
       (values ((nothing))) (expected (#f)) (expression? #t)))
    :expect #t)
  (test-submit
    ((alice-user journal-8 journal-9 journal-10 'put-batch!)
     '((paths ((*state* bob shared)))
       (values ("restored")) (expected ((nothing))) (expression? #t)))
    :expect #t)
  (test-submit
    ((alice-user journal-8 journal-9 journal-10 'put-batch!)
     '((paths ((*state* bob shared)))
       (values ("set-batch")) (expression? #t)))
    :expect #t)
  ;; Expected values require both get and set authority; unconditional writes
  ;; retain ordinary write-only authorization.
  (test-submit
    ((alice-user journal-8 journal-9 journal-10 'put-batch!)
     '((paths ((*state* bob write-only)))
       (values ("unconditional")) (expression? #t)))
    :expect #t)
  (test-submit
    ((alice-user journal-8 journal-9 journal-10 'put-batch!)
     '((paths ((*state* bob write-only)))
       (values ("must-not-write"))
       (expected ("unconditional")) (expression? #t)))
    :expect error-result?)
  ;; Every member is authorized before either batch read or write executes.
  (test-submit
    ((alice-user journal-8 journal-9 journal-10 'use-batch!)
     '((*state* bob shared) (*state* bob hidden)))
    :expect error-result?)
  (test-submit
    ((alice-user journal-8 journal-9 journal-10 'put-batch!)
     '((paths ((*state* bob shared) (*state* bob hidden)))
       (values ("must-not-write" "denied")) (expression? #t)))
    :expect error-result?)
  (test-submit
    ((alice-user journal-8 journal-9 journal-10 'put-batch!)
     '((paths ((*state* bob shared) (*state* bob shared)))
       (values ("malformed")) (expression? #t)))
    :expect error-result?)
  (test-submit
    ((alice-user journal-8 journal-9 journal-10 'use-batch!)
     (make-list 1025 '(*state* bob shared)))
    :expect error-result?)
  (test-submit ((alice-user journal-8 journal-9 journal-10 'bridge!) journal-11) :expect error-result?)
  (test-submit ((*journal* journal-9 'use!) '(*state* carol seed)) :expect "direct-batch")
  (test-submit ((*journal* journal-10 'use!) '(*state* bob shared)) :expect "set-batch")
  (test-submit ((*journal* journal-10 'use!) '(*state* bob write-only)) :expect "unconditional")
  (test-submit ((*journal* journal-10 'config) '((path (private bridge david)))) :expect '())

  (test-submit ((*journal* journal-10 'step!)) :expect 3)
  ;; Committed reads expose only journal-10 state already incorporated into journal-8's view.
  (test-submit ((*journal* journal-9 'step!)) :expect 4)
  (test-submit ((*journal* journal-8 'step!)) :expect 4)

  (test-submit ((alice-user journal-8 journal-9 journal-10 'retrieve) '(999 *state* bob shared)) :expect error-result?)
  (test-submit
    ((alice-user journal-8 journal-9 journal-10 'retrieve)
     '(-1 *state* bob shared) :pinned? #f :proof? #f)
    :expect "set-batch")
  (let* ((projected
          (collect
            ((alice-user journal-8 journal-9 journal-10 'retrieve)
             '(-1 *state* bob) :pinned? #f :proof? #t)))
         (proof (cadr (assoc 'proof projected))))
    (test-submit
      (exact journal-8
              `(*call* "pass-8"
                       (lambda (root)
                         (let ((ledger (sync-eval ((root 'get) '(root object ledger))))
                               (standard (sync-eval ((root 'get) '(root object standard)))))
                           ((ledger '~retrieve)
                            '(-1 journal-9 -1 journal-10 *state* bob hidden)
                            #f #f ((standard 'deserialize) ',proof))))))
      :expect '(unknown))
    (test-submit
      (exact journal-8
              `(*call* "pass-8"
                       (lambda (root)
                         (let ((ledger (sync-eval ((root 'get) '(root object ledger))))
                               (standard (sync-eval ((root 'get) '(root object standard)))))
                           ((ledger '~retrieve)
                            '(-1 journal-9 -1 journal-10 *state* bob *directory*)
                            #f #f ((standard 'deserialize) ',proof))))))
      :expect '(unknown)))
  ;; Authentication follows the canonical latest route while Ledger history
  ;; independently selects an older terminal checkpoint in the same traversal.
  (test-submit
    ((alice-user journal-8 journal-9 journal-10 'retrieve :history '(-1 -1 1))
     '(-1 *state* bob shared) :pinned? #f :proof? #f)
    :expect "initial")
  ;; Retention uses the same canonical route/history path. Interface obtains and
  ;; verifies proof internally, then mutates only the origin journal. Terminal
  ;; retrieve permission does not confer origin-local retention ownership.
  (test-submit
    ((alice-user journal-8 journal-9 journal-10 'pin!)
     '(-1 *state* bob shared))
    :expect error-result?)
  (test-submit
    ((alice-user journal-8 journal-9 journal-10 'unpin!)
     '(-1 *state* bob shared))
    :expect error-result?)
  (test-submit
    ((bob journal-8 journal-9 journal-10 'pin!)
     '(-1 *state* bob shared))
    :expect #t)
  (test-submit
    ((bob journal-8 journal-9 journal-10 'retrieve)
     '(-1 *state* bob shared) :pinned? #t :proof? #f)
    :expect '((content "set-batch") (pinned? #t)))
  (test-submit
    ((bob journal-8 journal-9 journal-10 'unpin!)
     '(-1 *state* bob shared))
    :expect #t)
  (test-submit
    ((bob journal-8 'pin!)
     '(-1 journal-9 -1 journal-10 *crypto* interface public-key))
    :expect error-result?)

  ;; Only the bridge initiator schedules the reciprocal exchange. The acceptor
  ;; stores the initiator head and later commits it during its own local step.
  (test-submit ((*journal* journal-8 'put!) :path '(*state* alice scheduled) :value "next") :expect #t)
  (test-submit ((*journal* journal-8 'step!)) :expect 5)
  (test-submit ((*journal* journal-9 'config) '((path (private bridge journal-8 last-index)))) :expect 3)

  ;; A remote rule without a signing-key window is rejected before it can
  ;; become an unusable policy entry.
  (test-submit
    ((*journal* journal-10 'authorize!)
     `((user (*state* bob))
       (rule ((principal ,remote-principal) (path (other))
              (use! ((read-only? #t))) (put! #f) (retrieve #f)))))
    :expect error-result?)
  (test-submit ((alice-user journal-8 journal-9 journal-10 'use!) '(*state* bob other)) :expect error-result?)

  ;; Interface-key rotation uses the key contained in journal-10's committed view.
  ;; A newly committed key is unusable until reciprocal synchronization carries
  ;; it into journal-10 and then carries that head back into journal-8 history.
  (define interface-8-secret-new "federation-a-new-secret")
  (test-submit ((*journal* journal-8 '*secret*) `((secret ,interface-8-secret-new))) :expect #t)
  (journal-8 'credentials interface-8-secret-new)
  (test-submit (federation-secret-free? journal-8 "pass-8") :expect #t)
  (test-submit (exact journal-8
                 `((function use!)
                   (arguments ((path (*state* bob shared)) (read-only? #t) (expression? #t)))
                   (invocation ((identity alice-user)
                                (route-source ())
                                (route-target (journal-9 journal-10))
                                (credentials ,interface-8-secret-new))))) :expect error-result?)
  (test-submit (exact journal-8
                 `((function use!)
                   (arguments ((path (*state* alice seed)) (read-only? #t) (expression? #t)))
                   (authentication ((credentials ,interface-8)))))
    :expect error-result?)
  (test-submit (exact journal-8
                 `((function put!)
                   (arguments ((path (*state* alice rotated))
                               (value "new-key")
                               (expression? #t)))
                   (authentication ((credentials ,interface-8-secret-new))))) :expect #t)
  (test-submit ((*journal* journal-8 'step!)) :expect 6)
  (test-submit (federation-secret-free? journal-8 "pass-8") :expect #t)
  (test-submit (exact journal-8
                 `((function use!)
                   (arguments ((path (*state* bob shared)) (read-only? #t) (expression? #t)))
                   (invocation ((identity alice-user)
                                (route-source ())
                                (route-target (journal-9 journal-10))
                                (credentials ,interface-8-secret-new)))))
    :expect error-result?)
  (test-submit (federation-secret-free? journal-8 "pass-8") :expect #t)
  (test-submit
    (interface-private-present? journal-8 "pass-8" interface-8-secret-new)
    :expect #f)

  ;; Normal reciprocal synchronization carries the replacement public key to
  ;; the terminal and then returns its committed view to the origin. The same
  ;; latest route becomes signable again without any old-key fallback.
  (test-submit (reciprocal-bridge journal-8 journal-9) :expect #t)
  (test-submit ((*journal* journal-9 'step!)) :expect integer?)
  (test-submit (reciprocal-bridge journal-9 journal-10) :expect #t)
  (test-submit ((*journal* journal-10 'step!)) :expect integer?)
  (test-submit (reciprocal-bridge journal-9 journal-10) :expect #t)
  (test-submit ((*journal* journal-9 'step!)) :expect integer?)
  (test-submit (reciprocal-bridge journal-8 journal-9) :expect #t)
  (test-submit ((*journal* journal-8 'step!)) :expect integer?)
  (test-submit (exact journal-8
                 `((function use!)
                   (arguments ((path (*state* bob shared)) (read-only? #t) (expression? #t)))
                   (invocation ((identity alice-user)
                                (route-source ())
                                (route-target (journal-9 journal-10))
                                (credentials ,interface-8-secret-new)))))
    :expect "set-batch")
  (test-submit (federation-secret-free? journal-8 "pass-8") :expect #t)

  ;; Federation-owned alias tables reject reader-opaque symbols before their
  ;; expression config or digest changes, including through public update-config!.
  (define federation-config-digest-before
    (collect (federation-digest journal-8 "pass-8")))
  (test-submit
    (federation-config-rejection journal-8 "pass-8" 'peers ".")
    :expect '(#t #t #t))
  (test-submit
    (federation-config-rejection journal-8 "pass-8" 'retired "two words")
    :expect '(#t #t #t))
  (test-submit
    (federation-config-set journal-8 "pass-8" 'peers "%2E"
                           '((marker encoded-dot)))
    :expect #t)
  (test-submit
    (federation-config-get journal-8 "pass-8" 'peers "%2E")
    :expect '((marker encoded-dot)))
  (test-submit
    (federation-config-set journal-8 "pass-8" 'retired "two%20words"
                           '((marker encoded-space)))
    :expect #t)
  (test-submit
    (federation-config-get journal-8 "pass-8" 'retired "two%20words")
    :expect '((marker encoded-space)))
  (test-submit
    (federation-config-set journal-8 "pass-8" 'peers "%2E" '())
    :expect #t)
  (test-submit
    (federation-config-set journal-8 "pass-8" 'retired "two%20words" '())
    :expect #t)
  (test-submit
    (federation-config-get journal-8 "pass-8" 'peers "%2E")
    :expect '())
  (test-submit
    (federation-config-get journal-8 "pass-8" 'retired "two%20words")
    :expect '())
  (test-submit
    (federation-digest journal-8 "pass-8")
    :expect federation-config-digest-before)

  ;; Complete alias-table replacements enforce the same admission before
  ;; mutation while retaining encoded names and empty-table replacement.
  (test-submit
    (federation-config-table-rejection journal-8 "pass-8" 'peers ".")
    :expect '(#t #t #t))
  (test-submit
    (federation-config-table-rejection journal-8 "pass-8" 'retired "two words")
    :expect '(#t #t #t))
  (test-submit
    (federation-config-table-set
     journal-8 "pass-8" 'peers
     '(("%2E" ((marker encoded-dot)))))
    :expect #t)
  (test-submit
    (federation-config-table-get journal-8 "pass-8" 'peers)
    :expect '((%2E ((marker encoded-dot)))))
  (test-submit
    (federation-config-table-set
     journal-8 "pass-8" 'retired
     '(("two%20words" ((marker encoded-space)))))
    :expect #t)
  (test-submit
    (federation-config-table-get journal-8 "pass-8" 'retired)
    :expect '((two%20words ((marker encoded-space)))))
  (test-submit
    (federation-config-table-set journal-8 "pass-8" 'peers '())
    :expect #t)
  (test-submit
    (federation-config-table-set journal-8 "pass-8" 'retired '())
    :expect #t)
  (test-submit
    (federation-config-table-get journal-8 "pass-8" 'peers)
    :expect '())
  (test-submit
    (federation-config-table-get journal-8 "pass-8" 'retired)
    :expect '())

  (test-report)))
